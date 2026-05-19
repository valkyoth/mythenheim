use axum::{Json, Router, routing::get};
use clap::Parser;
use mythenheim::{VERSION, config::AppConfig, db::migrations};
use serde::Serialize;
use std::{net::SocketAddr, path::PathBuf};
use tower_http::trace::TraceLayer;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[arg(long, default_value = "examples/mythenheim.toml")]
    config: PathBuf,

    #[arg(long)]
    check_config: bool,

    #[arg(long)]
    check_migrations: bool,

    #[arg(long)]
    print_migrations: bool,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    if cli.check_migrations {
        migrations::validate(migrations::all())?;
        println!("migrations ok: {} migration(s)", migrations::all().len());
        return Ok(());
    }

    if cli.print_migrations {
        print!("{}", migrations::render_all()?);
        return Ok(());
    }

    let config = AppConfig::load(&cli.config)?;

    if cli.check_config {
        println!("config ok: {}", cli.config.display());
        return Ok(());
    }

    let listen_addr: SocketAddr = config.server.listen_addr.parse()?;
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    tracing::info!(%listen_addr, "starting mythenheim");

    axum::serve(listener, app()).await?;
    Ok(())
}

fn app() -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .layer(TraceLayer::new_for_http())
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: VERSION,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn healthz_returns_ok() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
