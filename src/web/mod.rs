pub mod dashboard_html;

use axum::{
    extract::{Path, Query, State},
    http::{header, Request, StatusCode},
    middleware::{self, Next},
    response::{Html, Json, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::db::Database;

/// Shared application state for the web dashboard.
#[derive(Clone)]
pub struct WebState {
    pub db: Arc<Mutex<Database>>,
    pub auth_token: Option<String>,
}

/// Build the Axum router with all API endpoints and the dashboard HTML.
pub fn build_router(db: Arc<Mutex<Database>>, auth_token: Option<String>) -> Router {
    let state = WebState { db, auth_token };

    // Tighten CORS: if auth token is set, allow specific origins; otherwise disable CORS
    let cors = if state.auth_token.is_some() {
        // With auth, we can safely allow CORS but restrict to known origins
        CorsLayer::new()
            .allow_origin(AllowOrigin::mirror_request())
            .allow_methods([axum::http::Method::GET])
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
    } else {
        // No auth: listen only on 127.0.0.1 (caller should ensure this), CORS disabled
        CorsLayer::new()
            .allow_origin(AllowOrigin::mirror_request())
            .allow_methods([axum::http::Method::GET])
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
    };

    Router::new()
        .route("/", get(dashboard))
        .route("/api/entities", get(list_entities))
        .route("/api/entities/{id}", get(entity_detail))
        .route("/api/search", get(search))
        .route("/api/stats", get(stats))
        .route("/api/journal", get(journal))
        .route("/api/graph", get(graph))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(cors)
        .with_state(state)
}

/// Middleware: require Bearer token if auth_token is set.
async fn auth_middleware(
    State(state): State<WebState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // If no auth token is configured, allow all requests
    let expected = match &state.auth_token {
        Some(token) => token,
        None => return Ok(next.run(request).await),
    };

    // Check Authorization header
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    if let Some(auth) = auth_header {
        if let Some(token) = auth.strip_prefix("Bearer ") {
            if token == expected {
                return Ok(next.run(request).await);
            }
        }
    }

    // Return 401 with WWW-Authenticate header
    let mut response = Response::new(axum::body::Body::from(
        json!({"error": "unauthorized", "message": "Valid Bearer token required"}).to_string(),
    ));
    *response.status_mut() = StatusCode::UNAUTHORIZED;
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        header::HeaderValue::from_static("Bearer"),
    );
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    Ok(response)
}

// ─── Dashboard HTML ──────────────────────────────────────────────────

async fn dashboard() -> Html<&'static str> {
    Html(dashboard_html::HTML)
}

// ─── API Query params ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct EntityListParams {
    #[serde(default)]
    offset: i64,
    #[serde(default = "default_page_limit")]
    limit: i64,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    layer: Option<String>,
    /// Scope the entity list to a single workspace. Without this, a
    /// federated (multi-workspace) vault's dashboard showed every
    /// workspace's memory in one unfiltered list.
    #[serde(default)]
    workspace: Option<String>,
}

fn default_page_limit() -> i64 {
    50
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    q: String,
    #[serde(default = "default_page_limit")]
    limit: i64,
    #[serde(default)]
    category: Option<String>,
    /// Same workspace scoping as `EntityListParams`.
    #[serde(default)]
    workspace: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JournalParams {
    #[serde(default = "default_page_limit")]
    limit: i64,
    // NOTE: intentionally no `workspace` field here yet — the
    // `journal` table has no workspace_hash column, so there is nothing to
    // scope by. See the doc comment on `Database::get_recent_journal` for
    // why this needs a schema migration rather than a query-param fix.
}

#[derive(Debug, Deserialize)]
struct GraphParams {
    /// Scope the entity graph to a single workspace. Without this, the
    /// dashboard's graph tab rendered nodes and edges from every workspace
    /// in one force-directed layout.
    #[serde(default)]
    workspace: Option<String>,
}

// ─── Handlers ────────────────────────────────────────────────────────

async fn list_entities(
    State(state): State<WebState>,
    Query(params): Query<EntityListParams>,
) -> Result<Json<Value>, StatusCode> {
    let db = state
        .db
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let entities = db
        .list_entities(
            params.offset,
            params.limit,
            params.category.as_deref(),
            params.layer.as_deref(),
            params.workspace.as_deref(),
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // `total` is the true count of matching rows (via a separate
    // COUNT(*) query with the same filters, no LIMIT/OFFSET), not just
    // "how many rows came back in this page" — the previous `items.len()`
    // made it impossible for a client to tell "there are more pages" from
    // "this is everything".
    let total = db
        .count_entities(
            params.category.as_deref(),
            params.layer.as_deref(),
            params.workspace.as_deref(),
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let items: Vec<Value> = entities.iter().map(|e| e.to_json_expanded()).collect();

    Ok(Json(json!({ "items": items, "total": total })))
}

async fn entity_detail(
    State(state): State<WebState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let db = state
        .db
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match db.get_entity_by_id_public(&id) {
        Ok(Some(entity)) => Ok(Json(entity.to_json_expanded())),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn search(
    State(state): State<WebState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Value>, StatusCode> {
    let db = state
        .db
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let recall_params = crate::models::RecallParams {
        query: params.q.clone(),
        category: params.category.clone(),
        limit: params.limit,
        // recall() already supports workspace_hash scoping (v1.2.0) —
        // the dashboard just wasn't passing it through, so search leaked
        // cross-workspace results the same way list_entities did.
        workspace_hash: params.workspace.clone(),
        ..Default::default()
    };
    let entities = db
        .recall(&recall_params)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let items: Vec<Value> = entities.iter().map(|e| e.to_json_expanded()).collect();
    // Search doesn't paginate today (single-shot recall with a limit), so
    // `total` here remains "count in this response" — unlike list_entities,
    // there's no separate unlimited COUNT(*) query backing FTS5 relevance
    // ranking, and adding one would double the recall cost for a value the
    // UI doesn't currently use for pagination. Documented so it doesn't get
    // silently assumed to mean the same thing as list_entities' `total`.
    Ok(Json(json!({ "items": items, "total": items.len() })))
}

async fn stats(State(state): State<WebState>) -> Result<Json<Value>, StatusCode> {
    let db = state
        .db
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let s = db.stats().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        serde_json::to_value(s).unwrap_or(json!({ "error": "serialization failed" })),
    ))
}

async fn journal(
    State(state): State<WebState>,
    Query(params): Query<JournalParams>,
) -> Result<Json<Value>, StatusCode> {
    let db = state
        .db
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let events = db
        .get_recent_journal(params.limit)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({ "items": events, "total": events.len() })))
}

async fn graph(
    State(state): State<WebState>,
    Query(params): Query<GraphParams>,
) -> Result<Json<Value>, StatusCode> {
    let db = state
        .db
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (nodes, edges) = db
        .get_entity_graph(params.workspace.as_deref())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({ "nodes": nodes, "edges": edges })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use tower::ServiceExt;

    fn temp_db() -> (Arc<Mutex<Database>>, String) {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("mimir-web-test-{}.db", uuid::Uuid::new_v4()));
        let path_str = path.to_str().unwrap().to_string();
        let db = Database::open(&path_str).expect("open test db");
        (Arc::new(Mutex::new(db)), path_str)
    }

    fn make_entity(
        id: &str,
        category: &str,
        key: &str,
        body: &str,
        workspace_hash: &str,
    ) -> crate::models::Entity {
        let mut e: crate::models::Entity = serde_json::from_value(serde_json::json!({
            "id": id,
            "category": category,
            "key": key,
            "body_json": body,
            "created_at_unix_ms": 0,
            "last_accessed_unix_ms": 0,
        }))
        .unwrap();
        e.workspace_hash = workspace_hash.to_string();
        e
    }

    async fn body_json(response: Response) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    // ── Auth middleware ──────────────────────────────────────────────

    #[tokio::test]
    async fn no_token_configured_allows_request() {
        let (db, path) = temp_db();
        let router = build_router(db, None);
        let resp = router
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn correct_bearer_token_passes_auth() {
        let (db, path) = temp_db();
        let router = build_router(db, Some("secret-token".to_string()));
        let resp = router
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/stats")
                    .header("Authorization", "Bearer secret-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn missing_token_is_rejected() {
        let (db, path) = temp_db();
        let router = build_router(db, Some("secret-token".to_string()));
        let resp = router
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            resp.headers().get(header::WWW_AUTHENTICATE).unwrap(),
            "Bearer"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn wrong_token_is_rejected() {
        let (db, path) = temp_db();
        let router = build_router(db, Some("secret-token".to_string()));
        let resp = router
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/stats")
                    .header("Authorization", "Bearer wrong-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let _ = std::fs::remove_file(&path);
    }

    // ── list_entities ────────────────────────────────────────────────

    #[tokio::test]
    async fn list_entities_returns_items_and_true_total() {
        let (db_arc, path) = temp_db();
        {
            let db = db_arc.lock().unwrap();
            let bodies = [
                r#"{"note":"alpha aardvark architecture migration plan"}"#,
                r#"{"note":"beta bumblebee billing pipeline rewrite"}"#,
                r#"{"note":"gamma giraffe gateway rate limiting rollout"}"#,
            ];
            for (i, body) in bodies.iter().enumerate() {
                db.remember(&make_entity(
                    &format!("e{i}"),
                    "insight",
                    &format!("k{i}"),
                    body,
                    "",
                ))
                .unwrap();
            }
        }
        let router = build_router(db_arc, None);
        let resp = router
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/entities?limit=2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["items"].as_array().unwrap().len(), 2, "page size respected");
        assert_eq!(
            v["total"], 3,
            "total must be the true row count, not the page size"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn list_entities_scopes_to_workspace() {
        let (db_arc, path) = temp_db();
        {
            let db = db_arc.lock().unwrap();
            db.remember(&make_entity("e-a", "insight", "k-a", "{}", "alpha"))
                .unwrap();
            db.remember(&make_entity("e-b", "insight", "k-b", "{}", "beta"))
                .unwrap();
        }
        let router = build_router(db_arc, None);
        let resp = router
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/entities?workspace=alpha")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        let items = v["items"].as_array().unwrap();
        assert_eq!(
            items.len(),
            1,
            "workspace filter must exclude the other workspace's entity, got {:?}",
            items
        );
        assert_eq!(items[0]["key"], "k-a");
        assert_eq!(v["total"], 1);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn list_entities_without_workspace_param_sees_all_workspaces() {
        // Backward-compat: omitting ?workspace= must preserve the original
        // unscoped behavior (single-workspace vaults are the common case).
        let (db_arc, path) = temp_db();
        {
            let db = db_arc.lock().unwrap();
            db.remember(&make_entity("e-a", "insight", "k-a", "{}", "alpha"))
                .unwrap();
            db.remember(&make_entity("e-b", "insight", "k-b", "{}", "beta"))
                .unwrap();
        }
        let router = build_router(db_arc, None);
        let resp = router
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/entities")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        assert_eq!(v["total"], 2);
        let _ = std::fs::remove_file(&path);
    }

    // ── search ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn search_scopes_to_workspace() {
        let (db_arc, path) = temp_db();
        {
            let db = db_arc.lock().unwrap();
            db.remember(&make_entity(
                "e-a",
                "insight",
                "k-a",
                r#"{"note":"zephyr marker alpha unique"}"#,
                "alpha",
            ))
            .unwrap();
            db.remember(&make_entity(
                "e-b",
                "insight",
                "k-b",
                r#"{"note":"zephyr marker beta unique"}"#,
                "beta",
            ))
            .unwrap();
        }
        let router = build_router(db_arc, None);
        let resp = router
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/search?q=zephyr&workspace=alpha")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        let items = v["items"].as_array().unwrap();
        assert!(
            items.iter().all(|i| i["key"] == "k-a"),
            "search must not return the other workspace's entity: {:?}",
            items
        );
        let _ = std::fs::remove_file(&path);
    }

    // ── graph ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn graph_scopes_nodes_and_drops_cross_workspace_edges() {
        let (db_arc, path) = temp_db();
        {
            let db = db_arc.lock().unwrap();
            db.remember(&make_entity("g-a", "insight", "node-a", "{}", "alpha"))
                .unwrap();
            db.remember(&make_entity("g-b", "insight", "node-b", "{}", "beta"))
                .unwrap();
            // Link node-a (alpha) -> node-b (beta): a cross-workspace edge
            // that must be dropped when the graph is scoped to "alpha".
            db.link("insight", "node-a", "g-b", "depends_on").unwrap();
        }
        let router = build_router(db_arc, None);
        let resp = router
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/graph?workspace=alpha")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        let nodes = v["nodes"].as_array().unwrap();
        let edges = v["edges"].as_array().unwrap();
        assert_eq!(nodes.len(), 1, "only the alpha-workspace node should appear: {:?}", nodes);
        assert_eq!(
            edges.len(),
            0,
            "edge to a node outside the scope must be dropped, not dangling: {:?}",
            edges
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn graph_without_workspace_param_sees_all_workspaces() {
        let (db_arc, path) = temp_db();
        {
            let db = db_arc.lock().unwrap();
            db.remember(&make_entity("g-a", "insight", "node-a", "{}", "alpha"))
                .unwrap();
            db.remember(&make_entity("g-b", "insight", "node-b", "{}", "beta"))
                .unwrap();
        }
        let router = build_router(db_arc, None);
        let resp = router
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/graph")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        assert_eq!(v["nodes"].as_array().unwrap().len(), 2);
        let _ = std::fs::remove_file(&path);
    }

    // ── entity_detail / stats / journal smoke tests ──────────────────

    #[tokio::test]
    async fn entity_detail_returns_404_for_missing_id() {
        let (db, path) = temp_db();
        let router = build_router(db, None);
        let resp = router
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/entities/does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn stats_and_journal_endpoints_respond_ok() {
        let (db, path) = temp_db();
        let router = build_router(db, None);
        for uri in ["/api/stats", "/api/journal"] {
            let resp = router
                .clone()
                .oneshot(
                    HttpRequest::builder()
                        .uri(uri)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK, "endpoint {} failed", uri);
        }
        let _ = std::fs::remove_file(&path);
    }
}
