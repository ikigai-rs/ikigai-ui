//! Screens as s-expressions (ledger #3367): the structural front end (an ikigai-sexpr datum)
//! and the computed one (an ikigai-lisp program whose result is that datum), both onto the
//! outline's compile step; the three front ends agree; the model's s-expression face reads
//! back to the same model.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use ikigai_core::{
    ArgRef, Capability, Description, EndpointSpace, Error, Fallback, FnEndpoint, Invocation, Iri,
    Kernel, ReprType, Representation, Request, Scope, Space, UriTemplate, Verb,
};
use ikigai_ui::model::{self, Node, Value};
use ikigai_ui::{screen, sexpr};

/// Each screen's source: its media type and its text.
type Sources = Arc<Mutex<BTreeMap<String, (&'static str, String)>>>;

/// A host: ikigai-ui, ikigai-lisp's eval, a source per screen from `sources` (counting reads),
/// then the screens.
fn with(sources: &[(&str, &'static str, &str)]) -> (Kernel, Arc<AtomicUsize>) {
    let map: Sources = Arc::new(Mutex::new(
        sources
            .iter()
            .map(|(k, t, v)| (k.to_string(), (*t, v.to_string())))
            .collect(),
    ));
    let reads = Arc::new(AtomicUsize::new(0));
    let counted = reads.clone();
    let endpoint = FnEndpoint::new("test-screen-source", move |inv: &Invocation<'_>| {
        counted.fetch_add(1, Ordering::SeqCst);
        let name = inv.bindings.get("screen").unwrap_or_default();
        let (media_type, text) = map
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("no screen `{name}`")))?;
        Ok(Representation::new(ReprType::new(media_type), text.into_bytes()).cacheable())
    })
    .with_description(
        Description::new("test-screen-source")
            .verb(Verb::Source)
            .output(screen::OUTLINE)
            .output(sexpr::MEDIA_TYPE)
            .output(sexpr::LISP),
    );
    let sources = EndpointSpace::new().bind(
        UriTemplate::parse("urn:iki:ui:screen:name:{screen}:source").unwrap(),
        endpoint,
    );
    let kernel = Kernel::new(Arc::new(Fallback::new(vec![
        Arc::new(ikigai_ui::space()),
        Arc::new(ikigai_lisp::space()),
        Arc::new(sources),
        Arc::new(screen::screens()),
    ])));
    (kernel, reads)
}

fn request(name: &str) -> Request {
    Request::new(
        Verb::Source,
        Iri::parse(format!("{}{name}", screen::SCREEN_PREFIX)).unwrap(),
    )
}

fn read_as(kernel: &Kernel, name: &str, cap: &Capability, scope: Scope) -> Result<Node, Error> {
    let rep = block_on(kernel.issue_in(request(name), cap, scope))?;
    assert_eq!(rep.repr_type.media_type, model::MEDIA_TYPE);
    Ok(model::parse(&rep.bytes).unwrap())
}

fn read(kernel: &Kernel, name: &str) -> Result<Node, Error> {
    read_as(kernel, name, &Capability::root(), Scope::empty())
}

fn corridor(kernel: &Kernel) -> Scope {
    let space = screen::prefix_space(screen::PREFIXES);
    block_on(space.refresh(kernel, &Capability::root())).unwrap();
    Scope::empty().with(Arc::new(space) as Arc<dyn Space>)
}

fn refused(result: Result<Node, Error>) -> String {
    match result {
        Err(Error::InvalidArgument { detail, .. }) => detail,
        other => panic!("not refused as InvalidArgument: {other:?}"),
    }
}

const GALLERY_SEXPR: &str = r#"
; The gallery, as a datum.
(group :label "Gallery"
  (group :label "Text"
    (heading :content "Text" :level 2)
    (urn:iki:ui:example:kind:heading)
    (urn:iki:ui:example:kind:text)
    (urn:iki:ui:example:kind:status))
  (group :label "Input"
    (heading :content "Input" :level 2)
    (urn:iki:ui:example:kind:form)
    (urn:iki:ui:example:kind:field)
    (urn:iki:ui:example:kind:choice)
    (urn:iki:ui:example:kind:option)
    (urn:iki:ui:example:kind:button))
  (group :label "Navigation"
    (heading :content "Navigation" :level 2)
    (urn:iki:ui:example:kind:link)
    (urn:iki:ui:example:kind:nav))
  (group :label "Structure"
    (heading :content "Structure" :level 2)
    (urn:iki:ui:example:kind:list)
    (urn:iki:ui:example:kind:table)
    (urn:iki:ui:example:kind:group))
  (group :label "Media"
    (heading :content "Media" :level 2)
    (urn:iki:ui:example:kind:image)
    (urn:iki:ui:example:kind:visual)
    (urn:iki:ui:example:kind:scene)))
"#;

/// The gallery as a program: each section computed from its kinds' names.
const GALLERY_LISP: &str = r#"
(define (example k) (list 'urn:iki:ui:example:kind k))
(define (section label kinds)
  (append (list 'group ':label label (list 'heading ':content label ':level 2))
          (map example kinds)))
(append (list 'group ':label "Gallery")
  (list (section "Text" '("heading" "text" "status"))
        (section "Input" '("form" "field" "choice" "option" "button"))
        (section "Navigation" '("link" "nav"))
        (section "Structure" '("list" "table" "group"))
        (section "Media" '("image" "visual" "scene"))))
"#;

#[test]
fn the_gallery_is_one_model_across_outline_sexpr_and_program() {
    let outline = String::from_utf8(screen::SOURCES[0].bytes.to_vec()).unwrap();
    let (kernel, _) = with(&[
        ("outline", screen::OUTLINE, &outline),
        ("sexpr", sexpr::MEDIA_TYPE, GALLERY_SEXPR),
        ("lisp", sexpr::LISP, GALLERY_LISP),
    ]);
    let from_outline = read(&kernel, "outline").unwrap();
    let rep = block_on(kernel.issue(
        Request::new(Verb::Source, Iri::parse(ikigai_ui::GALLERY).unwrap()),
        &Capability::root(),
    ))
    .unwrap();
    assert_eq!(model::parse(&rep.bytes).unwrap(), from_outline);
    assert_eq!(read(&kernel, "sexpr").unwrap(), from_outline);
    assert_eq!(read(&kernel, "lisp").unwrap(), from_outline);
}

const CONTACT_OUTLINE: &str = "@prefixes urn:iki:ui:prefixes
group label=Contact
  heading content=\"Write to us\" level=1
  ui:example:kind:form description=\"We answer within a day\"
  ui:example:kind:status content=Recent
";

const CONTACT_SEXPR: &str = r#"(@prefixes urn:iki:ui:prefixes)
(group :label "Contact"
  (heading :content "Write to us" :level 1)
  (ui:example:kind:form :description "We answer within a day")
  (ui:example:kind:status :content Recent))
"#;

const CONTACT_LISP: &str = r#"
(define (ui name . overrides) (cons 'ui:example:kind (cons name overrides)))
(list 'group ':label "Contact"
  (list 'heading ':content "Write to us" ':level 1)
  (ui "form" ':description "We answer within a day")
  (ui "status" ':content "Recent"))
"#;

#[test]
fn the_contact_screen_and_its_prefixed_references_agree_across_the_three() {
    let (kernel, _) = with(&[
        ("outline", screen::OUTLINE, CONTACT_OUTLINE),
        ("sexpr", sexpr::MEDIA_TYPE, CONTACT_SEXPR),
    ]);
    let from_outline = read_as(&kernel, "outline", &Capability::root(), corridor(&kernel)).unwrap();
    assert_eq!(
        from_outline.children[1].properties["description"],
        Value::Text("We answer within a day".into())
    );
    let from_sexpr = read_as(&kernel, "sexpr", &Capability::root(), corridor(&kernel)).unwrap();
    assert_eq!(from_sexpr, from_outline);
    // Outside the corridor the datum's prefix table is not in force, refused as an outline's is.
    let detail = refused(read(&kernel, "sexpr"));
    assert!(
        detail.contains("the prefix table `urn:iki:ui:prefixes` is not in force"),
        "{detail}"
    );
    // A program's result is one datum: the last form's value. A program returning the prefixed
    // screen needs its `@prefixes` in the datum it returns, so this one returns the node alone
    // and its short names resolve in the corridor exactly as the datum's do.
    let (kernel, _) = with(&[("lisp", sexpr::LISP, CONTACT_LISP)]);
    let from_lisp = read_as(&kernel, "lisp", &Capability::root(), corridor(&kernel)).unwrap();
    assert_eq!(from_lisp, from_outline);
}

#[test]
fn an_override_the_kind_lacks_is_refused_naming_the_form() {
    let (kernel, _) = with(&[
        (
            "bad",
            sexpr::MEDIA_TYPE,
            r#"(group :label "G" (urn:iki:ui:example:kind:form :colour "red"))"#,
        ),
        (
            "worse",
            sexpr::LISP,
            r#"(list 'group ':label "G" (list 'urn:iki:ui:example:kind:form ':colour "red"))"#,
        ),
    ]);
    for name in ["bad", "worse"] {
        let detail = refused(read(&kernel, name));
        assert!(
            detail.contains(&format!(
                "urn:iki:ui:screen:name:{name} form 2: a form has no property `colour`"
            )),
            "{detail}"
        );
    }
}

#[test]
fn a_cycle_of_screens_through_sexpr_sources_is_refused() {
    let (kernel, _) = with(&[
        (
            "a",
            sexpr::MEDIA_TYPE,
            r#"(group :label "A" (urn:iki:ui:screen:name:b))"#,
        ),
        (
            "b",
            sexpr::LISP,
            r#"'(group :label "B" (urn:iki:ui:screen:name:a))"#,
        ),
    ]);
    let detail = refused(read(&kernel, "a"));
    assert!(detail.contains("a cycle of screens: a → b → a"), "{detail}");
}

#[test]
fn a_program_over_its_budget_is_refused_typed() {
    // Past ikigai-lisp's nesting budget: refused at its door, before Steel reads it.
    let deep = format!("{}1{}", "(list ".repeat(1001), ")".repeat(1001));
    let (kernel, _) = with(&[
        ("deep", sexpr::LISP, &deep),
        // Within the program's budgets, but its RESULT nests past what a screen may: refused by
        // the datum's own bound before the reader sees it.
        (
            "tall",
            sexpr::LISP,
            r#"(define (nest n) (if (= n 0) (list 'text ':content "x") (list 'group ':label "g" (nest (- n 1))))) (nest 40)"#,
        ),
    ]);
    let detail = refused(read(&kernel, "deep"));
    assert!(
        detail.contains("the program is refused (`in`)")
            && detail.contains("ikigai-lisp Limits::max_nesting"),
        "{detail}"
    );
    let detail = refused(read(&kernel, "tall"));
    assert!(
        detail.contains(&format!(
            "nested past the bound of {} lists",
            sexpr::MAX_NESTING
        )),
        "{detail}"
    );
}

#[test]
fn a_program_runs_under_the_callers_capability() {
    let (kernel, _) = with(&[("p", sexpr::LISP, r#"'(text :content "hi")"#)]);
    // Without `urn:cap:lisp` the caller may not run Lisp, so neither may its screen.
    let cap = Capability::scoped(["urn:cap:other"]);
    assert!(matches!(
        read_as(&kernel, "p", &cap, Scope::empty()),
        Err(Error::Denied(_))
    ));
    assert_eq!(
        read(&kernel, "p").unwrap().properties["content"],
        Value::Text("hi".into())
    );
}

#[test]
fn the_datum_is_bounded_before_it_is_read() {
    let deep = format!("{}{}", "(group :label \"g\" ".repeat(33), ")".repeat(33));
    let big = format!(
        "(text :content \"{}\")",
        "x".repeat(sexpr::MAX_SOURCE_BYTES)
    );
    let (kernel, _) = with(&[
        ("deep", sexpr::MEDIA_TYPE, &deep),
        ("big", sexpr::MEDIA_TYPE, &big),
        (
            "two",
            sexpr::MEDIA_TYPE,
            r#"(text :content "a") (text :content "b")"#,
        ),
        ("odd", sexpr::MEDIA_TYPE, r#"(text :content)"#),
        ("open", sexpr::MEDIA_TYPE, r#"(text :content "a""#),
        // Parentheses inside a string are text, not nesting.
        (
            "quoted",
            sexpr::MEDIA_TYPE,
            &format!("(text :content \"{}\")", "(".repeat(100)),
        ),
    ]);
    assert!(refused(read(&kernel, "deep")).contains("nested past the bound of 32 lists"));
    assert!(refused(read(&kernel, "big")).contains("past the bound of 65536"));
    assert!(refused(read(&kernel, "two")).contains("one root node"));
    assert!(refused(read(&kernel, "odd")).contains("form 1: `:content` has no value"));
    assert!(refused(read(&kernel, "open")).contains("form 0:"));
    assert_eq!(
        read(&kernel, "quoted").unwrap().properties["content"],
        Value::Text("(".repeat(100))
    );
}

#[test]
fn the_models_sexpr_face_reads_back_to_the_same_model() {
    let kernel = Kernel::new(Arc::new(ikigai_ui::space()));
    let face = block_on(
        kernel.issue(
            Request::new(Verb::Source, Iri::parse(ikigai_ui::GALLERY).unwrap())
                .with_arg("as", ArgRef::Inline(sexpr::MEDIA_TYPE.as_bytes().to_vec())),
            &Capability::root(),
        ),
    )
    .unwrap();
    assert_eq!(face.repr_type.media_type, sexpr::MEDIA_TYPE);
    let printed = String::from_utf8(face.bytes).unwrap();
    assert!(
        printed.starts_with("(group :label \"Gallery\"\n  (group"),
        "{printed}"
    );
    let rep = block_on(kernel.issue(
        Request::new(Verb::Source, Iri::parse(ikigai_ui::GALLERY).unwrap()),
        &Capability::root(),
    ))
    .unwrap();
    let gallery = model::parse(&rep.bytes).unwrap();
    assert_eq!(sexpr::print(&gallery).unwrap(), printed);
    // Read back (through the compiler, a datum with no references) it is the same model.
    // A flag the model holds as the text "true" reads back as the flag it means (the kind's
    // type, as an outline's would); otherwise the model is the same, and prints the same.
    let (kernel, _) = with(&[("printed", sexpr::MEDIA_TYPE, &printed)]);
    let back = read(&kernel, "printed").unwrap();
    let mut flagged = gallery.clone();
    let mut stack = vec![&mut flagged];
    while let Some(node) = stack.pop() {
        if let Some(Value::Text(t)) = node.properties.get("required") {
            let b = t == "true";
            node.properties.insert("required".into(), Value::Bool(b));
        }
        stack.extend(node.children.iter_mut());
    }
    assert_eq!(back, flagged);
    assert_eq!(
        sexpr::print(&back).unwrap(),
        sexpr::print(&flagged).unwrap()
    );
    // A model with a hint has no s-expression spelling: refused, never printed lossily.
    let mut hinted = gallery.clone();
    hinted.hint.insert(
        "urn:iki:ui:hint:x".into(),
        serde_json::from_str("\"y\"").unwrap(),
    );
    assert!(matches!(
        sexpr::print(&hinted),
        Err(Error::InvalidArgument { .. })
    ));
}

/// `urn:lisp:eval`, counting its runs: ikigai-lisp's own space, each hit's endpoint wrapped.
struct CountedEval {
    inner: Arc<dyn Space>,
    runs: Arc<AtomicUsize>,
}

impl Space for CountedEval {
    fn resolve(&self, request: &Request, scope: &Scope) -> ikigai_core::Resolution {
        let runs = self.runs.clone();
        self.inner.resolve(request, scope).map_endpoint(|inner| {
            let description = inner.describe();
            Arc::new(
                ikigai_core::AsyncFnEndpoint::new(
                    "counted-eval",
                    move |inv: &Invocation<'_>| -> ikigai_core::InvokeFuture<'_> {
                        runs.fetch_add(1, Ordering::SeqCst);
                        let inner = inner.clone();
                        Box::pin(async move { inner.invoke(inv).await })
                    },
                )
                .with_description(description),
            )
        })
    }
}

#[test]
fn a_program_with_cacheable_is_cached_and_one_without_it_is_re_run() {
    let ticks = Arc::new(AtomicUsize::new(0));
    let counted = ticks.clone();
    // A live resource: a new value on every read, never cacheable.
    let tick = EndpointSpace::new().bind(
        ikigai_core::Exact::new("urn:test:tick"),
        FnEndpoint::new("test-tick", move |_: &Invocation<'_>| {
            let n = counted.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(Representation::new(
                ReprType::new("text/plain"),
                n.to_string().into_bytes(),
            ))
        })
        .with_description(
            Description::new("test-tick")
                .verb(Verb::Source)
                .output("text/plain"),
        ),
    );
    let sources = EndpointSpace::new().bind(
        UriTemplate::parse("urn:iki:ui:screen:name:{screen}:source").unwrap(),
        FnEndpoint::new("test-screen-source", |inv: &Invocation<'_>| {
            let text = match inv.bindings.get("screen").unwrap_or_default() {
                "live" => r#"(list 'text ':content (source "urn:test:tick"))"#,
                "pure" => r#"(cacheable (list 'text ':content "fixed"))"#,
                "still" => r#"(list 'text ':content "fixed")"#,
                other => return Err(Error::NotFound(format!("no screen `{other}`"))),
            };
            Ok(
                Representation::new(ReprType::new(sexpr::LISP), text.as_bytes().to_vec())
                    .cacheable(),
            )
        })
        .with_description(
            Description::new("test-screen-source")
                .verb(Verb::Source)
                .output(sexpr::LISP),
        ),
    );
    let runs = Arc::new(AtomicUsize::new(0));
    let kernel = Kernel::new(Arc::new(Fallback::new(vec![
        Arc::new(ikigai_ui::space()),
        Arc::new(CountedEval {
            inner: Arc::new(ikigai_lisp::space()),
            runs: runs.clone(),
        }),
        Arc::new(tick),
        Arc::new(sources),
        Arc::new(screen::screens()),
    ])));
    let content = |name: &str| read(&kernel, name).unwrap().properties["content"].clone();

    // Live: the program reads a live resource, so every read of its screen runs it again.
    assert_eq!(content("live"), Value::Text("1".into()));
    assert_eq!(content("live"), Value::Text("2".into()));
    assert_eq!(content("live"), Value::Text("3".into()));
    assert_eq!(
        runs.load(Ordering::SeqCst),
        3,
        "the live program ran on every read"
    );

    // A program that does not declare `(cacheable …)` is live even when what it computes is
    // constant: its eval answers uncacheable, and so does its screen.
    runs.store(0, Ordering::SeqCst);
    assert_eq!(content("still"), Value::Text("fixed".into()));
    assert_eq!(content("still"), Value::Text("fixed".into()));
    assert_eq!(
        runs.load(Ordering::SeqCst),
        2,
        "an undeclared program re-runs"
    );

    // Declared pure: run once, then its screen is served from cache.
    runs.store(0, Ordering::SeqCst);
    assert_eq!(content("pure"), Value::Text("fixed".into()));
    assert_eq!(content("pure"), Value::Text("fixed".into()));
    assert_eq!(
        runs.load(Ordering::SeqCst),
        1,
        "a declared-pure program is cached"
    );
}
