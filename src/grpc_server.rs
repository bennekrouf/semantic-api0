use crate::endpoint_client::verify_endpoints_configuration;
use crate::models::config::load_server_config;
use crate::models::providers::ModelProvider;
use crate::progressive_matching::get_database_url;
use crate::sentence_service::sentence::sentence_service_server::SentenceServiceServer;
use crate::sentence_service::SentenceAnalyzeService;
use std::sync::Arc;
use tonic::transport::Server;
use tonic_reflection::server::Builder;
use tonic_web::GrpcWebLayer;
use tower_http::cors::{Any, CorsLayer};
use crate::app_log;
// In src/grpc_server.rs
pub async fn start_sentence_grpc_server(
    provider: Arc<dyn ModelProvider>,
    api_url: Option<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load server configuration
    let server_config = match load_server_config().await {
        Ok(config) => config,
        Err(e) => {
            app_log!(error, "Failed to load server configuration: {}", e);
            return Err(e);
        }
    };

    // Construct the address from config
    let server_addr = format!("{}:{}", server_config.address, server_config.port);
    let addr = server_addr.parse()?;

    app_log!(info, "Starting sentence analysis gRPC server on {}", addr);

    // Check if endpoints are available - REQUIRED for startup
    match verify_endpoints_configuration(api_url.clone()).await {
        Ok(true) => {
            app_log!(info, "Endpoint configuration verified - either remote service or local file is available");
        }
        Ok(false) => {
            app_log!(error, "FATAL: No endpoint configuration available! The server cannot start without endpoints.");
            return Err("No endpoint configuration available".into());
        }
        Err(e) => {
            app_log!(error, "Error checking endpoint configuration: {}", e);
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Failed to verify endpoint configuration: {e}"),
            )));
        }
    }

    app_log!(info, "Email is required for each request - no defaults will be used");

    let descriptor_set = include_bytes!(concat!(env!("OUT_DIR"), "/sentence_descriptor.bin"));
    let reflection_service = Builder::configure()
        .register_encoded_file_descriptor_set(descriptor_set)
        .build_v1()?;

    // Create CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers(Any)
        .allow_methods(Any)
        .expose_headers(Any);

    app_log!(info, "Starting semantic gRPC server on {}", addr);

    // Use the provider that was passed in from main.rs
    // In src/grpc_server.rs, change the initialization to:

    let sentence_service = match get_database_url() {
        Ok(db_url) => {
            app_log!(info, "Using database URL: {}", db_url);
            match SentenceAnalyzeService::with_progressive_matching(
                provider.clone(),
                api_url.clone(),
                &db_url,
            )
            .await
            {
                Ok(service) => service,
                Err(e) => {
                    app_log!(error, "Failed to initialize with progressive matching: {}", e);
                    app_log!(info, "Falling back to service without progressive matching");
                    SentenceAnalyzeService::new(provider, api_url)
                }
            }
        }
        Err(e) => {
            app_log!(error, "Failed to resolve database path: {}", e);
            app_log!(info, "Falling back to service without progressive matching");
            SentenceAnalyzeService::new(provider, api_url)
        }
    };
    let service = SentenceServiceServer::new(sentence_service);

    match Server::builder()
        .accept_http1(true)
        .max_concurrent_streams(128) // Set reasonable limits
        .tcp_keepalive(Some(std::time::Duration::from_secs(60)))
        .tcp_nodelay(true)
        .layer(cors) // Add CORS layer
        .layer(GrpcWebLayer::new())
        .add_service(service)
        .add_service(reflection_service) // Add reflection service
        .serve_with_shutdown(addr, async {
            tokio::signal::ctrl_c().await.ok();
            app_log!(info, "Shutting down semantic server...");
        })
        .await
    {
        Ok(_) => Ok::<(), Box<dyn std::error::Error + Send + Sync>>(()),
        Err(e) => {
            if e.to_string().contains("Address already in use") {
                app_log!(error, "Port already in use. Please stop other instances first.");
            }
            Err(e.into())
        }
    }
}
