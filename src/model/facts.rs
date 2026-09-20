use serde::{Deserialize, Serialize};

use super::LanguageId;
use super::SourceRange;

/// A syntactic scope: a region of source that lexically contains declarations.
///
/// Scopes exist to preserve containment. They are deliberately *not* semantic
/// namespaces and never claim to own a type's members: a Go method is not inside
/// its receiver type, a Rust `impl Type` block is not the declaration of `Type`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    /// Index of this scope inside its file analysis. Deterministic.
    pub scope_id: u32,
    pub parent_scope_id: Option<u32>,
    pub kind: ScopeKind,
    /// Written name when the construct is a named scope. `None` for anonymous
    /// callable scopes (closures, lambdas, arrow functions) and for scopes that
    /// do not declare the thing they enclose (Rust `impl` blocks). No synthetic
    /// names are invented.
    pub name: Option<String>,
    /// Range of the whole construct (including its body).
    pub range: SourceRange,
    /// Range of the construct header when it is distinguishable, e.g.
    /// `impl Foo` or `class Foo(Base)`. `None` when the header is the whole
    /// construct or cannot be separated.
    pub header_range: Option<SourceRange>,
    pub language: LanguageId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    /// Whole-file scope.
    File,
    Module,
    Package,
    Namespace,
    Struct,
    /// Rust `union`.
    Union,
    Class,
    Interface,
    Trait,
    Enum,
    /// Rust `impl` block. Not a declaration of the target type.
    Impl,
    /// Rust `extern` block. Contains foreign function signatures.
    ExternBlock,
    Function,
    Method,
    /// Anonymous function scope: Rust closure, Go func literal, Python lambda,
    /// PHP closure or arrow function.
    Closure,
    /// Python `lambda`.
    Lambda,
    /// PHP arrow function `fn () => ...`.
    ArrowFunction,
}

impl ScopeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ScopeKind::File => "file",
            ScopeKind::Module => "module",
            ScopeKind::Package => "package",
            ScopeKind::Namespace => "namespace",
            ScopeKind::Struct => "struct",
            ScopeKind::Union => "union",
            ScopeKind::Class => "class",
            ScopeKind::Interface => "interface",
            ScopeKind::Trait => "trait",
            ScopeKind::Enum => "enum",
            ScopeKind::Impl => "impl",
            ScopeKind::ExternBlock => "extern_block",
            ScopeKind::Function => "function",
            ScopeKind::Method => "method",
            ScopeKind::Closure => "closure",
            ScopeKind::Lambda => "lambda",
            ScopeKind::ArrowFunction => "arrow_function",
        }
    }
}

/// A syntactically declared construct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Declaration {
    /// Index of this declaration inside its file analysis. Deterministic.
    pub declaration_id: u32,
    /// Local snapshot identifier of the source file this declaration came from.
    pub snapshot_id: String,
    pub language: LanguageId,
    /// Path relative to the analysis root, always with `/` separators.
    pub relative_path: String,
    /// Lexically containing scope.
    pub scope_id: u32,
    pub kind: DeclarationKind,
    /// Written name exactly as it appears in the source (including `r#` raw
    /// identifier prefixes and PHP `$` sigils where the grammar includes them).
    pub name: String,
    pub name_range: SourceRange,
    /// Range of the whole declaration, including attributes/decorators,
    /// modifiers and body.
    pub range: SourceRange,
    /// Range of the body, when the construct has one. Never fabricated.
    pub body_range: Option<SourceRange>,
    pub flags: Vec<DeclarationFlag>,
    /// Declaration-local test evidence. Never a claim that a test runner would
    /// actually collect this declaration.
    pub test_evidence: Vec<TestEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationKind {
    Module,
    Package,
    Namespace,
    Function,
    Method,
    Struct,
    /// Rust `union`.
    Union,
    Class,
    Interface,
    Trait,
    Enum,
    /// `type X = Y` style alias (Rust `type`, Go `type X = Y`).
    TypeAlias,
    /// A nominal type definition that is not an alias and not a record/interface
    /// (Go `type X Y`).
    NamedType,
    Constant,
    Variable,
    Field,
    Property,
    /// Enum case / variant.
    Variant,
    /// Rust `macro_rules!` definition. Macros are never expanded.
    Macro,
}

impl DeclarationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DeclarationKind::Module => "module",
            DeclarationKind::Package => "package",
            DeclarationKind::Namespace => "namespace",
            DeclarationKind::Function => "function",
            DeclarationKind::Method => "method",
            DeclarationKind::Struct => "struct",
            DeclarationKind::Union => "union",
            DeclarationKind::Class => "class",
            DeclarationKind::Interface => "interface",
            DeclarationKind::Trait => "trait",
            DeclarationKind::Enum => "enum",
            DeclarationKind::TypeAlias => "type_alias",
            DeclarationKind::NamedType => "named_type",
            DeclarationKind::Constant => "constant",
            DeclarationKind::Variable => "variable",
            DeclarationKind::Field => "field",
            DeclarationKind::Property => "property",
            DeclarationKind::Variant => "variant",
            DeclarationKind::Macro => "macro",
        }
    }
}

/// Modifiers that are literally present in the source syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationFlag {
    Async,
    Static,
    Abstract,
    Final,
    Readonly,
    Const,
    /// A trait/interface method that ships a default body.
    Default,
    /// The declaration carries an explicit receiver (`self` in Rust, a Go
    /// method receiver).
    Receiver,
    /// PHP `__construct`.
    Constructor,
    /// PHP constructor-promoted property.
    Promoted,
    /// Rust `static mut`.
    Mutable,
    /// Rust `unsafe fn`.
    Unsafe,
    /// Variadic parameter list.
    Variadic,
}

impl DeclarationFlag {
    pub fn as_str(self) -> &'static str {
        match self {
            DeclarationFlag::Async => "async",
            DeclarationFlag::Static => "static",
            DeclarationFlag::Abstract => "abstract",
            DeclarationFlag::Final => "final",
            DeclarationFlag::Readonly => "readonly",
            DeclarationFlag::Const => "const",
            DeclarationFlag::Default => "default",
            DeclarationFlag::Receiver => "receiver",
            DeclarationFlag::Constructor => "constructor",
            DeclarationFlag::Promoted => "promoted",
            DeclarationFlag::Mutable => "mutable",
            DeclarationFlag::Unsafe => "unsafe",
            DeclarationFlag::Variadic => "variadic",
        }
    }
}

/// An import statement observed in the source, unresolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportOccurrence {
    pub import_id: u32,
    pub snapshot_id: String,
    pub language: LanguageId,
    pub relative_path: String,
    pub scope_id: u32,
    pub form: ImportForm,
    /// Common written prefix shared by every item, when the syntax has one:
    /// Rust `use a::b::{..}`, Python `from a.b import ..`, PHP `use A\B\{..}`.
    pub module: Option<String>,
    pub module_range: Option<SourceRange>,
    /// Number of leading dots for Python relative imports (`from ..pkg import x`
    /// is `2`). `None` when the statement is not a relative import.
    pub relative_levels: Option<u32>,
    pub items: Vec<ImportItem>,
    pub statement_range: SourceRange,
    /// Exact statement text, so nothing about the written form is lost.
    pub statement_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportForm {
    /// One item, no grouping syntax.
    Single,
    /// Braced/parenthesized group with several entries.
    Grouped,
    /// The statement imports everything from the module.
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportItem {
    /// Target exactly as written. For a wildcard entry this is `*`.
    pub target: String,
    pub target_range: SourceRange,
    pub alias: Option<String>,
    pub alias_range: Option<SourceRange>,
    pub wildcard: bool,
    pub category: ImportCategory,
    /// Range of the whole entry, including any alias.
    pub range: SourceRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportCategory {
    Normal,
    /// PHP `use function ...`.
    Function,
    /// PHP `use const ...`.
    Constant,
    /// Go blank import `_ "pkg"`.
    Blank,
    /// Go dot import `. "pkg"`.
    Dot,
}

impl ImportCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            ImportCategory::Normal => "normal",
            ImportCategory::Function => "function",
            ImportCategory::Constant => "constant",
            ImportCategory::Blank => "blank",
            ImportCategory::Dot => "dot",
        }
    }
}

impl ImportForm {
    pub fn as_str(self) -> &'static str {
        match self {
            ImportForm::Single => "single",
            ImportForm::Grouped => "grouped",
            ImportForm::Wildcard => "wildcard",
        }
    }
}

/// A local name binding introduced by syntax, with a bounded visibility region.
///
/// Source syntax introduces a local name that can shadow an outer name inside a
/// bounded source region. This is a *syntax* fact only: it records where the
/// written identifier is introduced and where it may apply, not that the name is
/// a call target, a variable of a particular type, or a resolved value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBindingOccurrence {
    /// Index of this binding inside its file analysis. Deterministic.
    pub binding_id: u32,
    /// Local snapshot identifier of the source file this binding came from.
    pub snapshot_id: String,
    pub language: LanguageId,
    /// Path relative to the analysis root, always with `/` separators.
    pub relative_path: String,
    /// Lexically containing scope (the enclosing function/closure/module/etc.).
    pub scope_id: u32,
    pub kind: BindingKind,
    /// Written bound identifier exactly as it appears in the source.
    pub name: String,
    /// Range of the bound identifier token.
    pub name_range: SourceRange,
    /// Range of the syntax that introduces the binding (the `let` statement,
    /// parameter, pattern node, or `let` condition).
    pub binding_site_range: SourceRange,
    /// Half-open source regions where the binding can syntactically shadow an
    /// outer name. Empty when the binding covers nothing (e.g. a parameter of
    /// a bodiless function signature).
    pub visibility_ranges: Vec<SourceRange>,
    /// `true` when this bare identifier sits in a refutable pattern position
    /// (`match` arm, `if let`, `while let`, `let`...`else`), where syntax alone
    /// cannot prove the name binds a fresh local rather than referencing a unit
    /// variant, constant, or path. The name is still recorded as a binding
    /// candidate, but a consumer may treat `ambiguous` bindings as uncertain
    /// rather than definite.
    pub ambiguous: bool,
}

impl LocalBindingOccurrence {
    /// `true` when `byte_offset` lies inside one of the binding's visibility
    /// ranges. A pure source-region test used to validate persisted regions —
    /// it is *not* name resolution and does not decide which binding wins when
    /// several cover the same position.
    pub fn covers(&self, byte_offset: u32) -> bool {
        self.visibility_ranges
            .iter()
            .any(|r| r.byte_start <= byte_offset && byte_offset < r.byte_end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingKind {
    /// `let` / `let`...`else` pattern binding.
    Let,
    /// Function or method parameter.
    FunctionParameter,
    /// Closure parameter.
    ClosureParameter,
    /// `for <pat> in ..` pattern binding.
    ForPattern,
    /// `match` arm pattern binding.
    MatchPattern,
    /// `if let` pattern binding.
    IfLetPattern,
    /// `while let` pattern binding.
    WhileLetPattern,
}

impl BindingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BindingKind::Let => "let",
            BindingKind::FunctionParameter => "function_parameter",
            BindingKind::ClosureParameter => "closure_parameter",
            BindingKind::ForPattern => "for_pattern",
            BindingKind::MatchPattern => "match_pattern",
            BindingKind::IfLetPattern => "if_let_pattern",
            BindingKind::WhileLetPattern => "while_let_pattern",
        }
    }
}

/// An unresolved, syntactically visible reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceOccurrence {
    pub reference_id: u32,
    pub kind: ReferenceKind,
    /// Written syntax, exactly as it appears in the source.
    pub written: String,
    pub range: SourceRange,
    /// Lexically containing scope.
    pub scope_id: u32,
    /// Declaration whose header this reference belongs to, when applicable.
    pub declaration_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    /// Rust `impl Foo` / `impl Trait for Foo` -> `Foo`.
    ImplTarget,
    /// Rust `impl Trait for Foo` -> `Trait`.
    ImplTrait,
    /// Go method receiver type.
    ReceiverType,
    /// Python base class, PHP `extends` target.
    BaseClass,
    /// PHP `implements` target.
    ImplementedInterface,
    /// PHP `use TraitName;` inside a class body (trait composition).
    TraitComposition,
    /// Go embedded struct/interface field type.
    EmbeddedType,
    /// PHP closure `use (...)` capture entry.
    ClosureCapture,
    /// PHP `include`/`require` target expression.
    IncludeTarget,
    /// PHP first-class callable syntax `foo(...)`. Not an invocation.
    FirstClassCallable,
    /// Python decorator name.
    Decorator,
}

impl ReferenceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ReferenceKind::ImplTarget => "impl_target",
            ReferenceKind::ImplTrait => "impl_trait",
            ReferenceKind::ReceiverType => "receiver_type",
            ReferenceKind::BaseClass => "base_class",
            ReferenceKind::ImplementedInterface => "implemented_interface",
            ReferenceKind::TraitComposition => "trait_composition",
            ReferenceKind::EmbeddedType => "embedded_type",
            ReferenceKind::ClosureCapture => "closure_capture",
            ReferenceKind::IncludeTarget => "include_target",
            ReferenceKind::FirstClassCallable => "first_class_callable",
            ReferenceKind::Decorator => "decorator",
        }
    }
}

/// A call-shaped expression. Never a resolved call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallLikeOccurrence {
    pub call_id: u32,
    pub scope_id: u32,
    pub form: CallLikeForm,
    /// Range of the whole call-shaped expression, including arguments.
    pub expression_range: SourceRange,
    /// Range of the callee sub-expression.
    pub callee_range: SourceRange,
    /// Callee exactly as written.
    pub callee_written: String,
    /// Written type arguments / generic instantiation, when present.
    pub type_arguments: Option<String>,
    pub type_arguments_range: Option<SourceRange>,
    /// The final name segment is a dynamic expression rather than a written
    /// identifier (PHP `$f()`, `$obj->{$m}()`).
    pub dynamic_callee: bool,
    /// PHP nullsafe `?->` call.
    pub nullsafe: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallLikeForm {
    /// `foo(...)`
    PlainName,
    /// `a::b::c(...)`, `a\b\c(...)`
    QualifiedPath,
    /// `obj.method(...)`, `$obj->m(...)`
    MemberSelector,
    /// `Type::m(...)`, `Type::m(...)`, `self::m(...)`
    StaticScoped,
    /// The callee is an arbitrary expression, not a written name/path/selector.
    Indirect,
    /// Go `T(x)` / `Generic[int](x)`. Tree-sitter's Go grammar classifies this
    /// shape as a type conversion, but the same syntax is exactly how a generic
    /// function is invoked with one argument. RepoDex records the occurrence
    /// and never decides which of the two it is.
    TypeConversion,
    /// PHP `new Foo(...)`.
    ExplicitConstruction,
    /// Rust `macro!(...)`. Macros are never expanded.
    MacroInvocation,
}

impl CallLikeForm {
    pub fn as_str(self) -> &'static str {
        match self {
            CallLikeForm::PlainName => "plain_name",
            CallLikeForm::QualifiedPath => "qualified_path",
            CallLikeForm::MemberSelector => "member_selector",
            CallLikeForm::StaticScoped => "static_scoped",
            CallLikeForm::Indirect => "indirect",
            CallLikeForm::TypeConversion => "type_conversion",
            CallLikeForm::ExplicitConstruction => "explicit_construction",
            CallLikeForm::MacroInvocation => "macro_invocation",
        }
    }
}

/// A syntactic observation suggesting a declaration may be a test.
///
/// This is evidence, not a claim about test-runner collection semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestEvidence {
    pub kind: TestEvidenceKind,
    /// The written syntax or convention that produced the evidence.
    pub detail: String,
    pub range: SourceRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestEvidenceKind {
    /// Rust `#[test]`, PHP `#[Test]`.
    Attribute,
    /// Python `@pytest.mark.*`.
    Decorator,
    /// `TestXxx` / `test_foo` naming.
    FunctionNameConvention,
    /// `TestFoo` class naming.
    ClassNameConvention,
    /// `*_test.go` / `test_*.py` file naming.
    FileNameConvention,
    /// Python `class X(unittest.TestCase)`.
    ClassBaseSyntax,
    /// Go `func TestXxx(t *testing.T)`.
    SignatureShape,
}

impl TestEvidenceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TestEvidenceKind::Attribute => "attribute",
            TestEvidenceKind::Decorator => "decorator",
            TestEvidenceKind::FunctionNameConvention => "function_name_convention",
            TestEvidenceKind::ClassNameConvention => "class_name_convention",
            TestEvidenceKind::FileNameConvention => "file_name_convention",
            TestEvidenceKind::ClassBaseSyntax => "class_base_syntax",
            TestEvidenceKind::SignatureShape => "signature_shape",
        }
    }
}
