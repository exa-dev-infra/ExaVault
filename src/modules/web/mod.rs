use askama::Template;
use axum::{
    extract::State,
    response::Redirect,
    routing::get,
    Router,
};
use axum_extra::extract::cookie::CookieJar;

use crate::{
    modules::vault::handler::VaultContext,
    pkg::crypto::verify_jwt,
};

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {}

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate {}

pub async fn root_redirect(
    State(ctx): State<VaultContext>,
    jar: CookieJar,
) -> Redirect {
    if let Some(c) = jar.get("exavault_session") {
        if verify_jwt(c.value(), &ctx.jwt_secret).is_ok() {
            return Redirect::to("/dashboard");
        }
    }
    Redirect::to("/login")
}

pub async fn login_view(
    State(ctx): State<VaultContext>,
    jar: CookieJar,
) -> Result<LoginTemplate, Redirect> {
    if let Some(c) = jar.get("exavault_session") {
        if verify_jwt(c.value(), &ctx.jwt_secret).is_ok() {
            return Err(Redirect::to("/dashboard"));
        }
    }
    Ok(LoginTemplate {})
}

pub async fn dashboard_view(
    State(ctx): State<VaultContext>,
    jar: CookieJar,
) -> Result<DashboardTemplate, Redirect> {
    let token = jar
        .get("exavault_session")
        .map(|c| c.value().to_string())
        .ok_or_else(|| Redirect::to("/login"))?;

    verify_jwt(&token, &ctx.jwt_secret).map_err(|_| Redirect::to("/login"))?;

    Ok(DashboardTemplate {})
}

pub fn web_router(ctx: VaultContext) -> Router {
    Router::new()
        .route("/", get(root_redirect))
        .route("/login", get(login_view))
        .route("/dashboard", get(dashboard_view))
        .with_state(ctx)
}
