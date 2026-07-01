// ===== Week4: Axum基礎 =====
//
// Axumの基本概念:
//   Router   : URLパスとハンドラ関数を対応付ける
//   Handler  : リクエストを受け取りレスポンスを返す async fn
//   Extractor: リクエストから必要な情報(パス・クエリ・Body・State等)を取り出す仕組み
//   State    : 全ハンドラで共有するデータ(DBプールなど)を渡す仕組み

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use tower_http::services::ServeDir;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool};

// ===== レスポンス/リクエストの型 =====
#[derive(Debug, Serialize, sqlx::FromRow)]
struct Deck {
    id:   i64,
    name: String,
}

// POSTリクエストのBodyを受け取る型
#[derive(Debug, Deserialize)]
struct CreateDeckBody {
    name: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct Card {
    id:      i64,
    deck_id: i64,
    front:   String,
    back:    String,
}

#[derive(Debug, Deserialize)]
struct CreateCardBody {
    front: String,
    back:  String,
}

// ===== アプリケーションの状態(全ハンドラで共有) =====
#[derive(Clone)]
struct AppState {
    pool: PgPool,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL が .env に設定されていません");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("DB接続に失敗しました");

    println!("DB接続成功");

    let state = AppState { pool };

    // Router: パスとHTTPメソッドごとにハンドラを登録する
    let app = Router::new()
        .route("/health",    get(health_handler))  // GET  /health
        .route("/decks",     get(list_decks))      // GET  /decks
        .route("/decks",     post(create_deck))    // POST /decks
        .route("/decks/:id",       delete(delete_deck))   // DELETE /decks/:id
        .route("/decks/:id/cards", get(list_cards))      // GET    /decks/:id/cards
        .route("/decks/:id/cards", post(create_card))    // POST   /decks/:id/cards
        .route("/cards/:id",       delete(delete_card))  // DELETE /cards/:id
        .route("/decks/:id/study", get(study_card))     // GET    /decks/:id/study
        .fallback_service(ServeDir::new("static"))      // static/以下のファイルを配信
        .with_state(state);                              // StateをRouterに渡す

    // Renderは環境変数PORTでポートを指定してくる。ローカルはデフォルト3000。
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap();

    println!("サーバー起動: http://localhost:{}", port);

    axum::serve(listener, app).await.unwrap();
}

// ===== ハンドラ =====

// GET /health: サーバーが動いているか確認するエンドポイント
async fn health_handler() -> impl IntoResponse {
    // Json(値) でJSON形式のレスポンスを返す
    Json(serde_json::json!({ "status": "ok" }))
}

// GET /decks: デッキ一覧を返す
// State(state): AppStateをAxumが自動で注入してくれる(Extractor)
async fn list_decks(State(state): State<AppState>) -> impl IntoResponse {
    let decks: Vec<Deck> = sqlx::query_as("SELECT id, name FROM decks ORDER BY id")
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

    Json(decks)
}

// POST /decks: デッキを作成する
// Json(body): リクエストBodyをCreateDeckBody構造体にデシリアライズして取り出す
async fn create_deck(
    State(state): State<AppState>,
    Json(body): Json<CreateDeckBody>,
) -> impl IntoResponse {
    let deck: Deck = sqlx::query_as(
        "INSERT INTO decks (name) VALUES ($1) RETURNING id, name"
    )
    .bind(&body.name)
    .fetch_one(&state.pool)
    .await
    .unwrap();

    // StatusCode::CREATED = HTTPステータス201(新規作成成功)
    (StatusCode::CREATED, Json(deck))
}

// DELETE /decks/:id: デッキを削除する
async fn delete_deck(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    sqlx::query("DELETE FROM decks WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .unwrap();

    StatusCode::NO_CONTENT
}

// GET /decks/:id/cards: 指定デッキのカード一覧を返す
async fn list_cards(
    State(state): State<AppState>,
    Path(deck_id): Path<i64>,
) -> impl IntoResponse {
    let cards: Vec<Card> = sqlx::query_as(
        "SELECT id, deck_id, front, back FROM cards WHERE deck_id = $1 ORDER BY id"
    )
    .bind(deck_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    Json(cards)
}

// POST /decks/:id/cards: 指定デッキにカードを追加する
async fn create_card(
    State(state): State<AppState>,
    Path(deck_id): Path<i64>,
    Json(body): Json<CreateCardBody>,
) -> impl IntoResponse {
    let card: Card = sqlx::query_as(
        "INSERT INTO cards (deck_id, front, back) VALUES ($1, $2, $3)
         RETURNING id, deck_id, front, back"
    )
    .bind(deck_id)
    .bind(&body.front)
    .bind(&body.back)
    .fetch_one(&state.pool)
    .await
    .unwrap();

    (StatusCode::CREATED, Json(card))
}

// GET /decks/:id/study: 指定デッキからランダムに1枚カードを返す
// fetch_optional: 0件なら None、1件なら Some(Card) を返す
async fn study_card(
    State(state): State<AppState>,
    Path(deck_id): Path<i64>,
) -> impl IntoResponse {
    let card: Option<Card> = sqlx::query_as(
        "SELECT id, deck_id, front, back FROM cards WHERE deck_id = $1 ORDER BY RANDOM() LIMIT 1"
    )
    .bind(deck_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap();

    // カードが存在する → 200 + JSON、存在しない → 404
    match card {
        Some(c) => (StatusCode::OK, Json(c)).into_response(),
        None    => StatusCode::NOT_FOUND.into_response(),
    }
}

// DELETE /cards/:id: カードを削除する
async fn delete_card(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    sqlx::query("DELETE FROM cards WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .unwrap();

    StatusCode::NO_CONTENT
}
