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
    Error, Expr, ExprArray, Ident, ItemFn, Lit, LitStr, MetaNameValue, PathArguments, Result,
    ReturnType, Type, parse_quote,
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

fn extract_success_type(output: &ReturnType) -> Result<Type> {
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
                            syn::GenericArgument::Type(ty) => Ok((*ty).clone()),
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
                    other @ PathArguments::Parenthesized(_) => Err(Error::new(
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
#[allow(clippy::too_many_lines, clippy::missing_panics_doc)]
pub fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args_tokens = parse_macro_input!(attr as ToolAttrInput);
    let args = match ToolArgs::parse(args_tokens.entries) {
        Ok(args) => args,
        Err(err) => return err.to_compile_error().into(),
    };

    let mut function = parse_macro_input!(item as ItemFn);

    if function.sig.asyncness.is_none() {
        return Error::new(function.sig.ident.span(), "tool functions must be async")
            .to_compile_error()
            .into();
    }

    let mut arguments = Vec::new();
    for arg in &function.sig.inputs {
        match arg {
            syn::FnArg::Typed(pat_type) => {
                let ident = match pat_type.pat.as_ref() {
                    syn::Pat::Ident(pat_ident) => pat_ident.ident.clone(),
                    other => {
                        return Error::new(
                            other.span(),
                            "tool parameters must be simple identifiers",
                        )
                        .to_compile_error()
                        .into();
                    }
                };
                arguments.push((ident, (*pat_type.ty).clone()));
            }
            syn::FnArg::Receiver(_) => {
                return Error::new(
                    function.sig.inputs.span(),
                    "tool functions cannot take `self` receivers",
                )
                .to_compile_error()
                .into();
            }
        }
    }
    if arguments.is_empty() {
        return Error::new(
            function.sig.span(),
            "tool functions must accept at least one argument",
        )
        .to_compile_error()
        .into();
    }

    let original_output = function.sig.output.clone();
    let success_ty = match extract_success_type(&original_output) {
        Ok(ty) => ty,
        Err(err) => return err.to_compile_error().into(),
    };

    function.sig.asyncness = None;
    let success_ty_future = success_ty.clone();
    function.sig.output = parse_quote!(-> ::agent_tools::registry::ToolFuture<#success_ty_future>);
    let original_body = function.block;
    function.block = Box::new(parse_quote!({
        ::std::boxed::Box::pin(async move #original_body)
    }));

    let fn_ident = &function.sig.ident;
    let binding_ident = format_ident!("{}_binding", fn_ident);
    let register_ident = format_ident!("register_{}", fn_ident);
    let mut const_name = fn_ident.to_string().to_uppercase();
    if !const_name.ends_with("_TOOL") {
        const_name.push_str("_TOOL");
    }
    let const_ident = Ident::new(&const_name, Span::call_site());
    let arg_types: Vec<Type> = arguments.iter().map(|(_, ty)| ty.clone()).collect();
    let success_ty_clone = success_ty.clone();

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

    let decode_arguments = if arguments.len() == 1 {
        let (ident, ty) = &arguments[0];
        quote! {
            let #ident: #ty = ::serde_json::from_value(input).map_err(|err| {
                ::agent_tools::registry::ToolError::execution(format!(
                    "failed to decode `{}` payload: {err}",
                    #name_lit,
                ))
            })?;
        }
    } else {
        let field_decoders = arguments.iter().map(|(ident, ty)| {
            let field_name = ident.to_string();
            quote! {
                let value = map.remove(#field_name).ok_or_else(|| {
                    ::agent_tools::registry::ToolError::execution(format!(
                        "tool `{}` missing field `{}`",
                        #name_lit,
                        #field_name,
                    ))
                })?;
                let #ident: #ty = ::serde_json::from_value(value).map_err(|err| {
                    ::agent_tools::registry::ToolError::execution(format!(
                        "failed to decode `{}` field `{}`: {err}",
                        #name_lit,
                        #field_name,
                    ))
                })?;
            }
        });
        quote! {
            let mut map = match input {
                ::serde_json::Value::Object(map) => map,
                other => {
                    return Err(::agent_tools::registry::ToolError::execution(format!(
                        "tool `{}` expects an object payload",
                        #name_lit,
                    )));
                }
            };
            #(#field_decoders)*
        }
    };
    let arg_idents: Vec<_> = arguments.iter().map(|(ident, _)| ident).collect();

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
                        #decode_arguments
                        let result: #success_ty = #fn_ident(#(#arg_idents),*).await?;
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

        #[allow(non_upper_case_globals)]
        #vis const #const_ident: ::agent_tools::registry::ToolDescriptor =
            ::agent_tools::registry::ToolDescriptor::new(#binding_ident);

        ::agent_tools::inventory::submit! {
            ::agent_tools::registry::ToolTypeRegistration::new(
                ::core::any::type_name::<fn(#(#arg_types),*) -> ::agent_tools::registry::ToolFuture<#success_ty_clone>>() ,
                ::agent_tools::registry::ToolDescriptor::new(#binding_ident),
            )
        }
    };

    TokenStream::from(expanded)
}
