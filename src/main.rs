use std::sync::Mutex;

use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};

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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // `HttpServer` accepts an application faactory instead of an application
    // instance. For shared mutable state, the object must be `Send + Sync`.
    // Internally, `web::Data` uses `Arc`, so to avoid creating multiple `Arc`,
    // create state before registering using `.app_data()`
    let counter = web::Data::new(AppStateWithCounter {
        counter: Mutex::new(0),
    });

    HttpServer::new(move || {
        App::new()
            .service(hello)
            .service(echo)
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
                    .service(app_visits),
            )
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
