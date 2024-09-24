use spin_sdk::http::{IntoResponse, Request, Response};
use spin_sdk::http_component;

mod bindings;

/// A simple Spin HTTP component.
#[http_component]
fn handle_example(req: Request) -> anyhow::Result<impl IntoResponse> {
    let output_body = bindings::deps::component::markdown_renderer::markdown_fns::render("Hello, Fermyon");

    println!("Handling request to {:?}", req.header("spin-full-url"));
    Ok(Response::builder()
        .status(200)
        .header("content-type", "text/plain")
        .body(output_body)
        .build())
}
