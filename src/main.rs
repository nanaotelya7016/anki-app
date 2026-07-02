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
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tower_http::services::ServeDir;

// ===== レスポンス/リクエストの型 =====

#[derive(Debug, Serialize, sqlx::FromRow)]
struct Deck {
    id:   i64,
    name: String,
}

#[derive(Debug, Deserialize)]
struct CreateDeckBody {
    name: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct Card {
    id:          i64,
    deck_id:     i64,
    front:       String,
    back:        String,
    // SRSフィールド
    easiness:    f64,      // SM-2の難易度係数(最小1.3、デフォルト2.5)
    interval:    i32,      // 次回出題までの日数
    repetitions: i32,      // 連続正解回数
    next_review: NaiveDate, // 次回出題日
}

#[derive(Debug, Deserialize)]
struct CreateCardBody {
    front: String,
    back:  String,
}

#[derive(Debug, Deserialize)]
struct ReviewBody {
    // 0: 全く覚えていない  3: 難しかったが思い出せた  5: 完璧に覚えている
    quality: i32,
}

// ===== SM-2アルゴリズム =====
//
// 間隔反復学習(SRS)の計算ロジック。
// 評価(quality)をもとに次回出題日を決める。
//
// 覚えていた(quality>=3): interval を伸ばす(1日→6日→EF倍...)
// 忘れた(quality<3)     : interval を1日にリセット
//
// EF(easiness factor): カードの難易度係数。正解するほど上がり、間違えると下がる。

struct Sm2Result {
    easiness:    f64,
    interval:    i32,
    repetitions: i32,
    next_review: NaiveDate,
}

fn sm2(easiness: f64, interval: i32, repetitions: i32, quality: i32) -> Sm2Result {
    let (new_repetitions, new_interval) = if quality >= 3 {
        // 正解: 連続正解回数を増やし、intervalを伸ばす
        let rep = repetitions + 1;
        let ivl = match repetitions {
            0 => 1,
            1 => 6,
            _ => (interval as f64 * easiness).round() as i32,
        };
        (rep, ivl)
    } else {
        // 不正解: リセット
        (0, 1)
    };

    // EFを更新(最小1.3)
    let new_ef = (easiness
        + 0.1
        - (5 - quality) as f64 * (0.08 + (5 - quality) as f64 * 0.02))
        .max(1.3);

    let next_review = chrono::Local::now().date_naive()
        + chrono::Duration::days(new_interval as i64);

    Sm2Result {
        easiness:    new_ef,
        interval:    new_interval,
        repetitions: new_repetitions,
        next_review,
    }
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

    // 起動時にマイグレーションを自動実行
    sqlx::migrate!().run(&pool).await.expect("マイグレーション失敗");
    println!("マイグレーション完了");

    let state = AppState { pool };

    let app = Router::new()
        .route("/health",           get(health_handler))
        .route("/decks",            get(list_decks))
        .route("/decks",            post(create_deck))
        .route("/decks/:id",        delete(delete_deck))
        .route("/decks/:id/cards",  get(list_cards))
        .route("/decks/:id/cards",  post(create_card))
        .route("/decks/:id/study",  get(study_card))
        .route("/cards/:id",        delete(delete_card))
        .route("/cards/:id/review", post(review_card))   // SRS評価
        .fallback_service(ServeDir::new("static"))
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

    println!("サーバー起動: http://localhost:{}", port);
    axum::serve(listener, app).await.unwrap();
}

// ===== ハンドラ =====

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn list_decks(State(state): State<AppState>) -> impl IntoResponse {
    let decks: Vec<Deck> = sqlx::query_as("SELECT id, name FROM decks ORDER BY id")
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
    Json(decks)
}

async fn create_deck(
    State(state): State<AppState>,
    Json(body): Json<CreateDeckBody>,
) -> impl IntoResponse {
    let deck: Deck = sqlx::query_as(
        "INSERT INTO decks (name) VALUES ($1) RETURNING id, name",
    )
    .bind(&body.name)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    (StatusCode::CREATED, Json(deck))
}

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

async fn list_cards(
    State(state): State<AppState>,
    Path(deck_id): Path<i64>,
) -> impl IntoResponse {
    let cards: Vec<Card> = sqlx::query_as(
        "SELECT id, deck_id, front, back, easiness, interval, repetitions, next_review
         FROM cards WHERE deck_id = $1 ORDER BY id",
    )
    .bind(deck_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    Json(cards)
}

async fn create_card(
    State(state): State<AppState>,
    Path(deck_id): Path<i64>,
    Json(body): Json<CreateCardBody>,
) -> impl IntoResponse {
    let card: Card = sqlx::query_as(
        "INSERT INTO cards (deck_id, front, back) VALUES ($1, $2, $3)
         RETURNING id, deck_id, front, back, easiness, interval, repetitions, next_review",
    )
    .bind(deck_id)
    .bind(&body.front)
    .bind(&body.back)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    (StatusCode::CREATED, Json(card))
}

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

// GET /decks/:id/study: 今日期限のカードを優先して出題
// 全カード復習済みの場合はランダムで出す
async fn study_card(
    State(state): State<AppState>,
    Path(deck_id): Path<i64>,
) -> impl IntoResponse {
    // 今日以前が期限のカードを優先(古いものから)
    let card: Option<Card> = sqlx::query_as(
        "SELECT id, deck_id, front, back, easiness, interval, repetitions, next_review
         FROM cards
         WHERE deck_id = $1 AND next_review <= CURRENT_DATE
         ORDER BY next_review ASC
         LIMIT 1",
    )
    .bind(deck_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap();

    // 期限カードがなければランダムで出す
    let card = if card.is_some() {
        card
    } else {
        sqlx::query_as(
            "SELECT id, deck_id, front, back, easiness, interval, repetitions, next_review
             FROM cards WHERE deck_id = $1 ORDER BY RANDOM() LIMIT 1",
        )
        .bind(deck_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap()
    };

    match card {
        Some(c) => (StatusCode::OK, Json(c)).into_response(),
        None    => StatusCode::NOT_FOUND.into_response(),
    }
}

// POST /cards/:id/review: SM-2で次回出題日を更新
async fn review_card(
    State(state): State<AppState>,
    Path(card_id): Path<i64>,
    Json(body): Json<ReviewBody>,
) -> impl IntoResponse {
    let card: Option<Card> = sqlx::query_as(
        "SELECT id, deck_id, front, back, easiness, interval, repetitions, next_review
         FROM cards WHERE id = $1",
    )
    .bind(card_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap();

    let card = match card {
        Some(c) => c,
        None    => return StatusCode::NOT_FOUND.into_response(),
    };

    let result = sm2(card.easiness, card.interval, card.repetitions, body.quality);

    sqlx::query(
        "UPDATE cards
         SET easiness=$1, interval=$2, repetitions=$3, next_review=$4
         WHERE id=$5",
    )
    .bind(result.easiness)
    .bind(result.interval)
    .bind(result.repetitions)
    .bind(result.next_review)
    .bind(card_id)
    .execute(&state.pool)
    .await
    .unwrap();

    StatusCode::OK.into_response()
}
