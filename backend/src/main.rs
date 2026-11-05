use anyhow::{Context, Result};
use jj_retail_api::{AppState, MIGRATOR, build_app, config::Config};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let config = Config::from_env()?;
    init_tracing(&config);

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("serve") => serve(config).await,
        Some("migrate") => {
            let pool = connect(&config).await?;
            MIGRATOR.run(&pool).await.context("running migrations")?;
            tracing::info!("migrations applied");
            Ok(())
        }
        Some("create-admin") => create_admin(config, &args[1..]).await,
        Some(other) => {
            anyhow::bail!("unknown command `{other}` (expected: serve | migrate | create-admin)")
        }
    }
}

fn init_tracing(config: &Config) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,sqlx=warn"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if config.log_format == "json" {
        builder.json().init();
    } else {
        builder.init();
    }
}

/// `create-admin --username <u> --display-name <name>`; password from
/// `JJ_ADMIN_PASSWORD` or an interactive prompt (not echoed).
async fn create_admin(config: Config, args: &[String]) -> Result<()> {
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let username = flag("--username").context("--username is required")?;
    let display_name = flag("--display-name").unwrap_or_else(|| username.clone());
    let password = match std::env::var("JJ_ADMIN_PASSWORD") {
        Ok(p) => p,
        Err(_) => {
            let p = rpassword::prompt_password("Password: ")?;
            if p != rpassword::prompt_password("Repeat password: ")? {
                anyhow::bail!("passwords do not match");
            }
            p
        }
    };
    let pool = connect(&config).await?;
    let id = jj_retail_api::admin::create_admin(&pool, &username, &display_name, &password)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("Created administrator {username} ({id})");
    Ok(())
}

async fn connect(config: &Config) -> Result<sqlx::PgPool> {
    PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&config.database_url)
        .await
        .context("connecting to PostgreSQL")
}

async fn serve(config: Config) -> Result<()> {
    let pool = connect(&config).await?;
    if config.run_migrations {
        MIGRATOR.run(&pool).await.context("running migrations")?;
    }
    let listener = tokio::net::TcpListener::bind(&config.bind)
        .await
        .with_context(|| format!("binding {}", config.bind))?;
    tracing::info!(bind = %config.bind, "jj-retail-api listening");
    tokio::spawn(jj_retail_api::inventory::check::nightly(pool.clone()));
    let app = build_app(AppState::new(pool, config));
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    tracing::info!("shutting down");
}
