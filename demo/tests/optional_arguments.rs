use std::sync::Arc;

use ikigai_core::{ArgRef, Capability, ContentId, Error, Iri, Request, Verb};
use ikigai_ui_demo::{service, Store, CONTACT};

fn inline(value: &str) -> ArgRef {
    ArgRef::Inline(value.as_bytes().to_vec())
}

#[tokio::test]
async fn contact_refuses_unreadable_fields_and_body_metadata() {
    let store: Store = Arc::default();
    let kernel = ikigai_ui_demo::kernel(Arc::new(service(store.clone())));
    let request = Request::new(Verb::Sink, Iri::parse(CONTACT).unwrap());
    let named = request
        .clone()
        .with_arg("name", inline("Ada"))
        .with_arg("email", inline("ada@example.org"))
        .with_arg("topic", inline("question"))
        .with_arg("message", inline("Hello"));
    let form = request.clone().with_arg(
        "content",
        inline("name=Ada&email=ada%40example.org&topic=question&message=Hello"),
    );
    let json = request
        .with_arg(
            "content",
            inline(
                r#"{"name":"Ada","email":"ada@example.org","topic":"question","message":"Hello"}"#,
            ),
        )
        .with_arg("content-type", inline("application/json; charset=utf-8"));
    for request in [named.clone(), form.clone(), json] {
        kernel.issue(request, &Capability::root()).await.unwrap();
    }

    let body = || ArgRef::Content(ContentId::of(b"body"));
    for (request, arg, value, reason) in [
        (&named, "content", body(), "inline"),
        (&named, "name", ArgRef::Inline(vec![0xff]), "UTF-8"),
        (&form, "email", body(), "inline"),
        (&form, "content", ArgRef::Inline(vec![0xff]), "UTF-8"),
        (&form, "content-type", ArgRef::Inline(vec![0xff]), "UTF-8"),
        (&form, "content-type", body(), "inline"),
        (
            &form,
            "content-type",
            inline("text/plain"),
            "use application/x-www-form-urlencoded or application/json",
        ),
    ] {
        match kernel
            .issue(request.clone().with_arg(arg, value), &Capability::root())
            .await
        {
            Err(Error::InvalidArgument { name, detail }) => {
                assert_eq!(name, arg, "{detail}");
                assert!(detail.contains(reason), "{arg}: {detail}");
            }
            other => panic!(
                "expected invalid {arg}, got {:?}",
                other.map(|r| r.repr_type)
            ),
        }
    }
    assert_eq!(store.lock().unwrap().len(), 3);
}
