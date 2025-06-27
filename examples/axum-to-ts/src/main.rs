use axum::{
    Json, Router,
    extract::Path,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/user", post(add_user))
        .route("/user/{id}", get(get_user));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[derive(Serialize, Deserialize)]
struct AddUserRequest {
    username: String,
}

#[derive(Serialize, Deserialize)]
struct User {
    id: u32,
    username: String,
}

async fn add_user(req: axum::extract::Json<AddUserRequest>) -> Json<User> {
    let user = User {
        id: 1,
        username: req.username.clone(),
    };
    axum::response::Json(user)
}

async fn get_user(Path(id): axum::extract::Path<u32>) -> Json<User> {
    let user = User {
        id,
        username: "example_user".to_string(),
    };
    Json(user)
}
