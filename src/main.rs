use actix_web::{
    body::BoxBody, error, get, http::header::ContentType, post, web, App, Error, HttpResponse, HttpServer, Responder
};
use serde::{Deserialize, Serialize};
use std::{sync::Mutex, time::Duration};

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hi there!")
}

#[post("/echo")]
async fn echo(req_body: String) -> impl Responder {
    HttpResponse::Ok().body(req_body)
}

async fn manual_hello() -> impl Responder {
    HttpResponse::Ok().body("Hi there!")
}

// Each worker thread processes requests sequentially, so handlers which
// block the current thread will cause the current thread to stop
// processing new requests. Thefefore, any long, non-cpu-bound operations
// (e.g. I/O, database operations, etc.) should be expressed as futures or
// asynchronous functions.
#[get("/wait")]
async fn wait() -> impl Responder {
    tokio::time::sleep(Duration::from_secs(5)).await; // worker thread will handle other requests
    HttpResponse::Ok().body("Waited 5 seconds!")
}

struct AppState {
    app_name: String,
}

// Application state is shared with all routes and resources **within the same
// scope**. State can be accessed with the `web::Data<T>` extractor. State is
// also accessible for middleware.
//
// State is registered to the root `App` instance or scope with `.app_data()`.
#[get("/index.html")]
async fn app_index(data: web::Data<AppState>) -> impl Responder {
    let app_name = &data.app_name; // Read state via reference.
    format!("Hello {app_name}!")
}

struct AppStateWithCounter {
    counter: Mutex<i32>, // Mutex allows safe mutation across threads.
}

#[get("/visits.html")]
async fn app_visits(data: web::Data<AppStateWithCounter>) -> impl Responder {
    let mut counter = data.counter.lock().unwrap();
    *counter += 1;
    format!("Visits so far: {counter}")
}

#[derive(Deserialize)]
struct AppPathInfo {
    user_id: u32,
    friend: String,
}

// Request information can be retrieved safely with _extractors_. Actix Web
// supports up to 12 extractors per handler function, and argument position
// does not matter.
//
// `web::Path` provides information extracted from the request's path. Parts of
// the path that are extractable ("dynamic segments") are marked with curly
// braces and retrieved as a tuple in the order of definition.
//
// It is also possible to extract path information to a type that implements
// `serde::Deserialize`.
#[get("/users/{user_id}/{friend}")]
async fn app_path(
    path: web::Path<(u32, String)>,
    struct_path: web::Path<AppPathInfo>,
) -> impl Responder {
    let (user_id, friend) = path.into_inner();
    _ = struct_path.user_id;
    _ = struct_path.friend;
    HttpResponse::Ok().body(format!("Welcome {}, user_id {}!", friend, user_id))
}

#[derive(Deserialize)]
struct AppQueryParams {
    username: String,
}

// `web::Query` provides extraction functionality for query parameters.
#[get("/query")]
async fn app_query(query: web::Query<AppQueryParams>) -> impl Responder {
    format!("Welcome {}!", query.username)
}

#[derive(Deserialize)]
struct AppSubmitInfo {
    username: String,
    password: String,
}

// `web::Json` allows deserialization of a request body into a struct. To
// extract typed information from a request body, the type `T` must implement
// `serde::Deserialize`.
//
// Some extractors like `web::Json` allow configuration of the extraction process.
// Pass the configuration object into `.app_data()`. In the case of `web::Json`,
// use `web::JsonConfig`.
//
// URL-Encoded forms can be extracted with `web::Form` and configured with
// `web::FormConfig`.
#[post("/submit")]
async fn app_submit(body: web::Json<AppSubmitInfo>) -> impl Responder {
    format!(
        "Submitting for {} with password {}!",
        body.username, body.password
    )
}

// To return a custom type directly from a handler function, it needs to
// implement the `Responder` trait.
#[derive(Serialize)]
struct AppResponse {
    username: String,
}

impl Responder for AppResponse {
    type Body = BoxBody;

    fn respond_to(self, _req: &actix_web::HttpRequest) -> HttpResponse<Self::Body> {
        let body = serde_json::to_string(&self).unwrap();

        HttpResponse::Ok()
            .content_type(ContentType::json())
            .body(body)
    }
}

#[get("/profile/{username}")]
async fn app_profile(username: web::Path<String>) -> impl Responder {
    AppResponse { username: username.to_string() }
}

use futures::{future, stream};

// The response body can also be generated asynchronously. In this case,
// the body must implement `Stream<Item = Result<Bytes, Error>>` and the
// response is called with `.streaming()`.
#[get("/stream")]
async fn app_stream() -> impl Responder {
    let body = stream::once(future::ok::<_, Error>(web::Bytes::from_static(b"streamed body")));

    HttpResponse::Ok()
        .content_type(ContentType::json())
        .streaming(body)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // `HttpServer` accepts an application factory instead of an application
    // instance. For shared mutable state, the object must be `Send + Sync`.
    // Internally, `web::Data` uses `Arc`, so to avoid creating multiple `Arc`,
    // create state before registering using `.app_data()`
    let counter = web::Data::new(AppStateWithCounter {
        counter: Mutex::new(0),
    });

    // Application state doesn't need to be `Send` or `Sync` but application
    // factories must be `Send + Sync`.
    HttpServer::new(move || {
        App::new()
            .service(hello)
            .service(echo)
            .service(wait)
            // Register a Responder manually without Actix macros.
            .route("/hey", web::get().to(manual_hello))
            // Register a service within a scope, which adds a prefix to all
            // resources and routes attached to it.
            .service(
                web::scope("/app")
                    // Register state to the scope.
                    .app_data(web::Data::new(AppState {
                        app_name: String::from("Actix Web"),
                    }))
                    .app_data(counter.clone()) // Internally `Arc`.
                    .service(app_index)
                    .service(app_visits)
                    .service(app_path)
                    .service(app_query)
                    .app_data(
                        web::JsonConfig::default()
                            .limit(4096)
                            .error_handler(|err, _req| {
                                error::InternalError::from_response(
                                    err,
                                    HttpResponse::Conflict().finish(),
                                )
                                .into()
                            }),
                    )
                    .service(app_submit)
                    .service(app_profile),
            )
    })
    .bind(("127.0.0.1", 8080))?
    // `HttpServer` starts a number of HTTP _workers_, by default equal in
    // number to the number of physical CPUs in the system. This can be
    // overridden with the `HttpServer::workers()` method.
    .workers(8)
    .run()
    // The server must be `await`ed or `spawn`ed to start processing requests
    // and will run until it receives a shutdown signal `ctrl-c`.
    .await
}
