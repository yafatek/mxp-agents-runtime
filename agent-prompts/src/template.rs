//! Code-based prompt template system with variable substitution.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Result alias for template operations.
pub type TemplateResult<T> = Result<T, TemplateError>;

/// Errors that can occur during template operations.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    /// A required variable was not provided.
    #[error("missing required variable: {name}")]
    MissingVariable {
        /// Name of the missing variable.
        name: String,
    },

    /// Template rendering failed.
    #[error("template rendering failed: {reason}")]
    RenderError {
        /// Reason for the failure.
        reason: String,
    },
}

/// A code-based prompt template with variable substitution.
///
/// Templates support simple `{{variable}}` syntax for variable substitution.
/// Variables can be required or optional with defaults.
///
/// # Examples
///
/// ```
/// use agent_prompts::template::{PromptTemplate, TemplateBuilder};
///
/// let template = TemplateBuilder::new("You are {{role}}. {{task}}")
///     .with_variable("role", "a helpful assistant")
///     .with_variable("task", "Answer questions concisely.")
///     .build()
///     .unwrap();
///
/// let rendered = template.render().unwrap();
/// assert!(rendered.contains("helpful assistant"));
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromptTemplate {
    template: String,
    variables: HashMap<String, String>,
    required_variables: Vec<String>,
}

impl PromptTemplate {
    /// Creates a new template with the supplied text.
    #[must_use]
    pub fn new(template: impl Into<String>) -> Self {
        Self {
            template: template.into(),
            variables: HashMap::new(),
            required_variables: Vec::new(),
        }
    }

    /// Returns a builder for constructing templates.
    #[must_use]
    pub fn builder(template: impl Into<String>) -> TemplateBuilder {
        TemplateBuilder::new(template)
    }

    /// Sets a variable value.
    pub fn set_variable(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.variables.insert(name.into(), value.into());
    }

    /// Returns the value of a variable if set.
    #[must_use]
    pub fn get_variable(&self, name: &str) -> Option<&str> {
        self.variables.get(name).map(String::as_str)
    }

    /// Renders the template with the current variables.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::MissingVariable`] if a required variable is not set.
    pub fn render(&self) -> TemplateResult<String> {
        self.render_with(&HashMap::new())
    }

    /// Renders the template with additional runtime variables.
    ///
    /// Runtime variables override template variables.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::MissingVariable`] if a required variable is not set.
    pub fn render_with(&self, runtime_vars: &HashMap<String, String>) -> TemplateResult<String> {
        let mut result = self.template.clone();

        // Extract all variable references from the template
        let var_refs = extract_variable_refs(&result);

        for var_name in var_refs {
            let value = runtime_vars
                .get(&var_name)
                .or_else(|| self.variables.get(&var_name));

            let value = if let Some(v) = value {
                v
            } else {
                if self.required_variables.contains(&var_name) {
                    return Err(TemplateError::MissingVariable {
                        name: var_name.clone(),
                    });
                }
                // Optional variable, replace with empty string
                ""
            };

            let placeholder = format!("{{{{{var_name}}}}}");
            result = result.replace(&placeholder, value);
        }

        Ok(result)
    }

    /// Returns the raw template string.
    #[must_use]
    pub fn template(&self) -> &str {
        &self.template
    }

    /// Returns the configured variables.
    #[must_use]
    pub fn variables(&self) -> &HashMap<String, String> {
        &self.variables
    }
}

impl fmt::Display for PromptTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.template)
    }
}

/// Builder for constructing prompt templates.
pub struct TemplateBuilder {
    template: String,
    variables: HashMap<String, String>,
    required_variables: Vec<String>,
}

impl TemplateBuilder {
    /// Creates a new builder with the supplied template text.
    #[must_use]
    pub fn new(template: impl Into<String>) -> Self {
        Self {
            template: template.into(),
            variables: HashMap::new(),
            required_variables: Vec::new(),
        }
    }

    /// Sets a variable with a default value.
    #[must_use]
    pub fn with_variable(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.variables.insert(name.into(), value.into());
        self
    }

    /// Declares a required variable (must be provided at render time).
    #[must_use]
    pub fn with_required_variable(mut self, name: impl Into<String>) -> Self {
        self.required_variables.push(name.into());
        self
    }

    /// Builds the template.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError::RenderError`] if the template is invalid.
    pub fn build(self) -> TemplateResult<PromptTemplate> {
        Ok(PromptTemplate {
            template: self.template,
            variables: self.variables,
            required_variables: self.required_variables,
        })
    }
}

/// Extracts variable names from a template string.
fn extract_variable_refs(template: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let mut chars = template.chars().peekable();
    let mut in_var = false;
    let mut current_var = String::new();
    let mut brace_count = 0;

    while let Some(ch) = chars.next() {
        if ch == '{' {
            if chars.peek() == Some(&'{') {
                chars.next(); // consume second brace
                in_var = true;
                brace_count = 2;
                current_var.clear();
            }
        } else if ch == '}' && in_var {
            if chars.peek() == Some(&'}') {
                chars.next(); // consume second brace
                brace_count -= 2;
                if brace_count == 0 {
                    in_var = false;
                    if !current_var.is_empty() {
                        vars.push(current_var.trim().to_owned());
                        current_var.clear();
                    }
                }
            }
        } else if in_var {
            current_var.push(ch);
        }
    }

    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_simple_template() {
        let template = PromptTemplate::builder("Hello {{name}}!")
            .with_variable("name", "World")
            .build()
            .unwrap();

        let rendered = template.render().unwrap();
        assert_eq!(rendered, "Hello World!");
    }

    #[test]
    fn renders_multiple_variables() {
        let template = PromptTemplate::builder("{{greeting}} {{name}}, {{question}}")
            .with_variable("greeting", "Hello")
            .with_variable("name", "Alice")
            .with_variable("question", "how are you?")
            .build()
            .unwrap();

        let rendered = template.render().unwrap();
        assert_eq!(rendered, "Hello Alice, how are you?");
    }

    #[test]
    fn runtime_variables_override_defaults() {
        let template = PromptTemplate::builder("Hello {{name}}!")
            .with_variable("name", "World")
            .build()
            .unwrap();

        let mut runtime = HashMap::new();
        runtime.insert("name".to_owned(), "Alice".to_owned());

        let rendered = template.render_with(&runtime).unwrap();
        assert_eq!(rendered, "Hello Alice!");
    }

    #[test]
    fn required_variables_error_when_missing() {
        let template = PromptTemplate::builder("Hello {{name}}!")
            .with_required_variable("name")
            .build()
            .unwrap();

        let err = template.render().expect_err("should error");
        assert!(matches!(err, TemplateError::MissingVariable { .. }));
    }

    #[test]
    fn extracts_variable_refs() {
        let template = "Hello {{name}}, you are {{age}} years old. {{greeting}}";
        let vars = extract_variable_refs(template);
        assert_eq!(vars.len(), 3);
        assert!(vars.contains(&"name".to_owned()));
        assert!(vars.contains(&"age".to_owned()));
        assert!(vars.contains(&"greeting".to_owned()));
    }

    #[test]
    fn handles_nested_braces() {
        let template = "Code: {{code}}";
        let vars = extract_variable_refs(template);
        assert_eq!(vars, vec!["code"]);
    }

    #[test]
    fn mutable_template_updates() {
        let mut template = PromptTemplate::new("Hello {{name}}!");
        template.set_variable("name", "Bob");

        let rendered = template.render().unwrap();
        assert_eq!(rendered, "Hello Bob!");
    }
}
