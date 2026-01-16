use syn::{visit::Visit, Expr, ExprCall, ExprLit, ExprMethodCall, Lit};
use std::collections::HashMap;

/// Result of label extraction from a test file
pub struct LabelExtractionResult {
    /// Map from test function name to its labels
    pub labels_by_test: HashMap<String, Vec<String>>,
    /// Errors encountered during extraction
    pub errors: Vec<LabelExtractionError>,
}

#[derive(Debug, Clone)]
pub struct LabelExtractionError {
    pub test_fn_name: String,
    pub line: usize,
    pub message: String,
}

/// Extract labels from a test file using AST parsing
pub fn extract_labels_from_file(content: &str) -> LabelExtractionResult {
    let mut result = LabelExtractionResult {
        labels_by_test: HashMap::new(),
        errors: Vec::new(),
    };

    // Parse the file
    let syntax = match syn::parse_file(content) {
        Ok(s) => s,
        Err(e) => {
            result.errors.push(LabelExtractionError {
                test_fn_name: "<parse error>".to_string(),
                line: 0,
                message: format!("Failed to parse file: {}", e),
            });
            return result;
        }
    };

    // Find all test functions
    for item in &syntax.items {
        if let syn::Item::Fn(func) = item {
            // Check if function has #[test] attribute
            let has_test_attr = func.attrs.iter().any(|attr| {
                attr.path().is_ident("test")
            });

            if has_test_attr {
                let fn_name = func.sig.ident.to_string();
                let mut visitor = LabelVisitor {
                    labels: Vec::new(),
                    errors: Vec::new(),
                    current_fn_name: fn_name.clone(),
                };

                visitor.visit_block(&func.block);

                result.labels_by_test.insert(fn_name, visitor.labels);
                result.errors.extend(visitor.errors);
            }
        }
    }

    result
}

/// Visitor that walks the AST and finds spec_expect/expect_msg calls
struct LabelVisitor {
    labels: Vec<String>,
    errors: Vec<LabelExtractionError>,
    current_fn_name: String,
}

impl<'ast> Visit<'ast> for LabelVisitor {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        match expr {
            // Method calls: scenario.spec_expect(...) or scenario.until(...).spec_expect(...)
            Expr::MethodCall(method_call) => {
                self.check_method_call(method_call);
                // Continue visiting nested expressions
                syn::visit::visit_expr_method_call(self, method_call);
            }
            // Function calls: spec_expect(...) [less common but supported]
            Expr::Call(call) => {
                self.check_function_call(call);
                // Continue visiting nested expressions
                syn::visit::visit_expr_call(self, call);
            }
            _ => {
                // Visit other expressions recursively
                syn::visit::visit_expr(self, expr);
            }
        }
    }
}

impl LabelVisitor {
    fn check_method_call(&mut self, method_call: &ExprMethodCall) {
        let method_name = method_call.method.to_string();

        if method_name == "spec_expect" || method_name == "expect_msg" {
            // Extract the first argument
            if let Some(first_arg) = method_call.args.first() {
                match self.extract_string_literal(first_arg) {
                    Ok(label) => {
                        self.labels.push(label);
                    }
                    Err(err_msg) => {
                        self.errors.push(LabelExtractionError {
                            test_fn_name: self.current_fn_name.clone(),
                            line: 0, // Line number not easily accessible from span
                            message: format!(
                                "Label must be a string literal for tooling in {}(); {}",
                                method_name, err_msg
                            ),
                        });
                    }
                }
            } else {
                self.errors.push(LabelExtractionError {
                    test_fn_name: self.current_fn_name.clone(),
                    line: 0, // Line number not easily accessible from span
                    message: format!("{}() missing first argument (label)", method_name),
                });
            }
        }
    }

    fn check_function_call(&mut self, call: &ExprCall) {
        // Check if this is a direct function call to spec_expect or expect_msg
        if let Expr::Path(path) = &*call.func {
            if let Some(ident) = path.path.get_ident() {
                let fn_name = ident.to_string();
                if fn_name == "spec_expect" || fn_name == "expect_msg" {
                    if let Some(first_arg) = call.args.first() {
                        match self.extract_string_literal(first_arg) {
                            Ok(label) => {
                                self.labels.push(label);
                            }
                            Err(err_msg) => {
                                self.errors.push(LabelExtractionError {
                                    test_fn_name: self.current_fn_name.clone(),
                                    line: 0, // Line number not easily accessible from span
                                    message: format!(
                                        "Label must be a string literal for tooling in {}(); {}",
                                        fn_name, err_msg
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    fn extract_string_literal(&self, expr: &Expr) -> Result<String, String> {
        match expr {
            Expr::Lit(ExprLit {
                lit: Lit::Str(lit_str),
                ..
            }) => Ok(lit_str.value()),
            _ => Err("expected string literal, found expression".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_line_spec_expect() {
        let content = r#"
#[test]
fn test_example() {
    scenario.spec_expect("connection-01.t1: connects", |ctx| Some(()));
}
"#;
        let result = extract_labels_from_file(content);
        assert_eq!(result.errors.len(), 0, "Should have no errors");
        assert_eq!(result.labels_by_test.len(), 1);
        let labels = &result.labels_by_test["test_example"];
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0], "connection-01.t1: connects");
    }

    #[test]
    fn test_multi_line_spec_expect() {
        let content = r#"
#[test]
fn test_example() {
    scenario.spec_expect(
        "connection-01.t1: connects",
        |ctx| Some(())
    );
}
"#;
        let result = extract_labels_from_file(content);
        assert_eq!(result.errors.len(), 0, "Should have no errors");
        let labels = &result.labels_by_test["test_example"];
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0], "connection-01.t1: connects");
    }

    #[test]
    fn test_method_chain_with_until() {
        let content = r#"
#[test]
fn test_example() {
    scenario.until(Duration::from_secs(5)).spec_expect("connection-01.t1: connects", |ctx| Some(()));
}
"#;
        let result = extract_labels_from_file(content);
        assert_eq!(result.errors.len(), 0, "Should have no errors");
        let labels = &result.labels_by_test["test_example"];
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0], "connection-01.t1: connects");
    }

    #[test]
    fn test_expect_msg() {
        let content = r#"
#[test]
fn test_example() {
    scenario.expect_msg("messaging-01.t1: receives message", |ctx| {
        ctx.client(key, |c| c.has_message())
    });
}
"#;
        let result = extract_labels_from_file(content);
        assert_eq!(result.errors.len(), 0, "Should have no errors");
        let labels = &result.labels_by_test["test_example"];
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0], "messaging-01.t1: receives message");
    }

    #[test]
    fn test_non_literal_label_error() {
        let content = r#"
#[test]
fn test_example() {
    let label = "connection-01.t1: connects";
    scenario.spec_expect(label, |ctx| Some(()));
}
"#;
        let result = extract_labels_from_file(content);
        assert_eq!(result.errors.len(), 1, "Should have one error");
        assert!(result.errors[0].message.contains("must be a string literal"));
        let labels = &result.labels_by_test["test_example"];
        assert_eq!(labels.len(), 0, "Should extract no labels");
    }

    #[test]
    fn test_multiple_labels_in_one_test() {
        let content = r#"
#[test]
fn test_example() {
    scenario.spec_expect("connection-01.t1: connects", |ctx| Some(()));
    scenario.spec_expect("connection-01.t2: handshake", |ctx| Some(()));
    scenario.expect_msg("messaging-01.t1: message", |ctx| Some(()));
}
"#;
        let result = extract_labels_from_file(content);
        assert_eq!(result.errors.len(), 0, "Should have no errors");
        let labels = &result.labels_by_test["test_example"];
        assert_eq!(labels.len(), 3);
        assert_eq!(labels[0], "connection-01.t1: connects");
        assert_eq!(labels[1], "connection-01.t2: handshake");
        assert_eq!(labels[2], "messaging-01.t1: message");
    }
}
