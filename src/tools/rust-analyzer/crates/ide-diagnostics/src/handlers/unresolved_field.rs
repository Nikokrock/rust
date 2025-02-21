use hir::{db::ExpandDatabase, HirDisplay, InFile};
use ide_db::{
    assists::{Assist, AssistId, AssistKind},
    base_db::FileRange,
    label::Label,
    source_change::SourceChange,
};
use syntax::{ast, AstNode, AstPtr};
use text_edit::TextEdit;

use crate::{adjusted_display_range, Diagnostic, DiagnosticCode, DiagnosticsContext};

// Diagnostic: unresolved-field
//
// This diagnostic is triggered if a field does not exist on a given type.
pub(crate) fn unresolved_field(
    ctx: &DiagnosticsContext<'_>,
    d: &hir::UnresolvedField,
) -> Diagnostic {
    let method_suffix = if d.method_with_same_name_exists {
        ", but a method with a similar name exists"
    } else {
        ""
    };
    Diagnostic::new(
        DiagnosticCode::RustcHardError("E0559"),
        format!(
            "no field `{}` on type `{}`{method_suffix}",
            d.name.display(ctx.sema.db),
            d.receiver.display(ctx.sema.db)
        ),
        adjusted_display_range(ctx, d.expr, &|expr| {
            Some(
                match expr {
                    ast::Expr::MethodCallExpr(it) => it.name_ref(),
                    ast::Expr::FieldExpr(it) => it.name_ref(),
                    _ => None,
                }?
                .syntax()
                .text_range(),
            )
        }),
    )
    .with_fixes(fixes(ctx, d))
    .experimental()
}

fn fixes(ctx: &DiagnosticsContext<'_>, d: &hir::UnresolvedField) -> Option<Vec<Assist>> {
    if d.method_with_same_name_exists {
        method_fix(ctx, &d.expr)
    } else {
        // FIXME: add quickfix

        None
    }
}

// FIXME: We should fill out the call here, move the cursor and trigger signature help
fn method_fix(
    ctx: &DiagnosticsContext<'_>,
    expr_ptr: &InFile<AstPtr<ast::Expr>>,
) -> Option<Vec<Assist>> {
    let root = ctx.sema.db.parse_or_expand(expr_ptr.file_id);
    let expr = expr_ptr.value.to_node(&root);
    let FileRange { range, file_id } = ctx.sema.original_range_opt(expr.syntax())?;
    Some(vec![Assist {
        id: AssistId("expected-field-found-method-call-fix", AssistKind::QuickFix),
        label: Label::new("Use parentheses to call the method".to_string()),
        group: None,
        target: range,
        source_change: Some(SourceChange::from_text_edit(
            file_id,
            TextEdit::insert(range.end(), "()".to_owned()),
        )),
        trigger_signature_help: false,
    }])
}
#[cfg(test)]
mod tests {
    use crate::{
        tests::{check_diagnostics, check_diagnostics_with_config},
        DiagnosticsConfig,
    };

    #[test]
    fn smoke_test() {
        check_diagnostics(
            r#"
fn main() {
    ().foo;
    // ^^^ error: no field `foo` on type `()`
}
"#,
        );
    }

    #[test]
    fn method_clash() {
        check_diagnostics(
            r#"
struct Foo;
impl Foo {
    fn bar(&self) {}
}
fn foo() {
    Foo.bar;
     // ^^^ 💡 error: no field `bar` on type `Foo`, but a method with a similar name exists
}
"#,
        );
    }

    #[test]
    fn method_trait_() {
        check_diagnostics(
            r#"
struct Foo;
trait Bar {
    fn bar(&self) {}
}
impl Bar for Foo {}
fn foo() {
    Foo.bar;
     // ^^^ 💡 error: no field `bar` on type `Foo`, but a method with a similar name exists
}
"#,
        );
    }

    #[test]
    fn method_trait_2() {
        check_diagnostics(
            r#"
struct Foo;
trait Bar {
    fn bar(&self);
}
impl Bar for Foo {
    fn bar(&self) {}
}
fn foo() {
    Foo.bar;
     // ^^^ 💡 error: no field `bar` on type `Foo`, but a method with a similar name exists
}
"#,
        );
    }

    #[test]
    fn no_diagnostic_on_unknown() {
        check_diagnostics(
            r#"
fn foo() {
    x.foo;
    (&x).foo;
    (&((x,),),).foo;
}
"#,
        );
    }

    #[test]
    fn no_diagnostic_for_missing_name() {
        let mut config = DiagnosticsConfig::test_sample();
        config.disabled.insert("syntax-error".to_owned());
        check_diagnostics_with_config(config, "fn foo() { (). }");
    }
}
