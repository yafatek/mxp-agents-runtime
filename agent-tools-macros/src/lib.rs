//! Procedural macros for agent tool definitions.
//!
//! The `#[tool]` attribute decorates async functions and generates the
//! registration glue required for the runtime to expose them to LLM adapters.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::parse_macro_input;
use syn::spanned::Spanned;
use syn::{
    Error, Expr, ExprArray, ItemFn, Lit, LitStr, MetaNameValue, PathArguments, Result, ReturnType,
    Type,
};

#[derive(Default)]
struct ToolArgs {
    name: Option<LitStr>,
    version: Option<LitStr>,
    description: Option<LitStr>,
    capabilities: Vec<LitStr>,
}

impl ToolArgs {
    fn parse(args: Vec<MetaNameValue>) -> Result<Self> {
        let mut parsed = ToolArgs::default();
        for arg in args {
            let MetaNameValue { path, value, .. } = arg;
            if path.is_ident("name") {
                parsed.name = Some(expect_lit_str(value, "name")?);
            } else if path.is_ident("version") {
                parsed.version = Some(expect_lit_str(value, "version")?);
            } else if path.is_ident("description") {
                parsed.description = Some(expect_lit_str(value, "description")?);
            } else if path.is_ident("capabilities") {
                parsed.capabilities = parse_capabilities(value)?;
            } else {
                return Err(Error::new(
                    path.span(),
                    "unsupported attribute key; expected one of `name`, `version`, `description`, or `capabilities`",
                ));
            }
        }

        if parsed.name.is_none() {
            return Err(Error::new(
                Span::call_site(),
                "missing required attribute `name`",
            ));
        }

        if parsed.version.is_none() {
            return Err(Error::new(
                Span::call_site(),
                "missing required attribute `version`",
            ));
        }

        Ok(parsed)
    }
}

fn expect_lit_str(expr: Expr, field: &str) -> Result<LitStr> {
    match expr {
        Expr::Lit(syn::ExprLit {
            lit: Lit::Str(lit), ..
        }) => Ok(lit),
        other => Err(Error::new(
            other.span(),
            format!("`{field}` must be a string literal"),
        )),
    }
}

fn parse_capabilities(expr: Expr) -> Result<Vec<LitStr>> {
    match expr {
        Expr::Array(ExprArray { elems, .. }) => {
            let mut caps = Vec::with_capacity(elems.len());
            for elem in elems {
                caps.push(expect_lit_str(elem, "capabilities entry")?);
            }
            Ok(caps)
        }
        other => Err(Error::new(
            other.span(),
            "`capabilities` must be an array of string literals",
        )),
    }
}

fn extract_success_type(output: &ReturnType) -> Result<&Type> {
    match output {
        ReturnType::Type(_, ty) => match ty.as_ref() {
            Type::Path(path) => {
                let last = path.path.segments.last().ok_or_else(|| {
                    Error::new(path.span(), "unsupported return type for tool function")
                })?;
                if last.ident != "ToolResult" {
                    return Err(Error::new(
                        last.ident.span(),
                        "tool functions must return agent_tools::registry::ToolResult<T>",
                    ));
                }
                match &last.arguments {
                    PathArguments::AngleBracketed(args) => {
                        if args.args.len() != 1 {
                            return Err(Error::new(
                                args.span(),
                                "ToolResult must contain exactly one generic argument",
                            ));
                        }
                        match &args.args[0] {
                            syn::GenericArgument::Type(ty) => Ok(ty),
                            other => Err(Error::new(
                                other.span(),
                                "ToolResult generic argument must be a concrete type",
                            )),
                        }
                    }
                    PathArguments::None => Err(Error::new(
                        last.arguments.span(),
                        "ToolResult must specify a success type",
                    )),
                    other => Err(Error::new(
                        other.span(),
                        "unsupported ToolResult generic arguments",
                    )),
                }
            }
            other => Err(Error::new(
                other.span(),
                "unsupported return type for tool function",
            )),
        },
        ReturnType::Default => Err(Error::new(
            Span::call_site(),
            "tool functions must return agent_tools::registry::ToolResult<T>",
        )),
    }
}

struct ToolAttrInput {
    entries: Vec<MetaNameValue>,
}

impl Parse for ToolAttrInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut entries = Vec::new();
        while !input.is_empty() {
            entries.push(input.parse()?);
            if input.peek(syn::Token![,]) {
                let _ = input.parse::<syn::Token![,]>()?;
            }
        }
        Ok(Self { entries })
    }
}

#[proc_macro_attribute]
pub fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args_tokens = parse_macro_input!(attr as ToolAttrInput);
    let args = match ToolArgs::parse(args_tokens.entries) {
        Ok(args) => args,
        Err(err) => return err.to_compile_error().into(),
    };

    let function = parse_macro_input!(item as ItemFn);

    if function.sig.asyncness.is_none() {
        return Error::new(function.sig.ident.span(), "tool functions must be async")
            .to_compile_error()
            .into();
    }

    if function.sig.inputs.len() != 1 {
        return Error::new(
            function.sig.inputs.span(),
            "tool functions must accept exactly one argument representing the input payload",
        )
        .to_compile_error()
        .into();
    }

    let input_ty = match function.sig.inputs.first().unwrap() {
        syn::FnArg::Typed(pat_type) => pat_type.ty.as_ref(),
        syn::FnArg::Receiver(_) => {
            return Error::new(
                function.sig.inputs.span(),
                "tool functions cannot take `self` receivers",
            )
            .to_compile_error()
            .into();
        }
    };

    let success_ty = match extract_success_type(&function.sig.output) {
        Ok(ty) => ty,
        Err(err) => return err.to_compile_error().into(),
    };

    let fn_ident = &function.sig.ident;
    let binding_ident = format_ident!("{}_binding", fn_ident);
    let register_ident = format_ident!("register_{}", fn_ident);
    let vis = &function.vis;

    let name_lit = args.name.expect("name checked above");
    let version_lit = args.version.expect("version checked above");

    let description_stmt = args.description.map(|desc| {
        quote! {
            metadata = metadata.with_description(#desc);
        }
    });

    let capabilities_stmt = if args.capabilities.is_empty() {
        quote! {}
    } else {
        let caps: Vec<_> = args
            .capabilities
            .iter()
            .map(|cap| {
                let value = cap.value();
                quote! {
                    ::agent_primitives::CapabilityId::new(#cap)
                        .map_err(|err| ::agent_tools::registry::ToolError::InvalidMetadata {
                            reason: format!("invalid capability `{}`: {err}", #value),
                        })?
                }
            })
            .collect();
        quote! {
            metadata = metadata.with_capabilities(vec![#(#caps),*]);
        }
    };

    let expanded = quote! {
        #function

        #vis fn #binding_ident() -> ::agent_tools::registry::ToolResult<::agent_tools::registry::ToolBinding> {
            let mut metadata = ::agent_tools::registry::ToolMetadata::new(#name_lit, #version_lit)?;
            #description_stmt
            #capabilities_stmt

            Ok(::agent_tools::registry::ToolBinding::new(
                metadata,
                |input: ::serde_json::Value| -> ::agent_tools::registry::ToolFuture {
                    ::std::boxed::Box::pin(async move {
                        let payload: #input_ty = ::serde_json::from_value(input).map_err(|err| {
                            ::agent_tools::registry::ToolError::execution(format!(
                                "failed to decode `{}` payload: {err}",
                                #name_lit,
                            ))
                        })?;
                        let result: #success_ty = #fn_ident(payload).await?;
                        let json = ::serde_json::to_value(result).map_err(|err| {
                            ::agent_tools::registry::ToolError::execution(format!(
                                "failed to encode `{}` response: {err}",
                                #name_lit,
                            ))
                        })?;
                        Ok(json)
                    })
                },
            ))
        }

        #vis fn #register_ident(
            registry: &::agent_tools::registry::ToolRegistry,
        ) -> ::agent_tools::registry::ToolResult<()> {
            let binding = #binding_ident()?;
            registry.register_binding(binding)
        }
    };

    TokenStream::from(expanded)
}
