        Some(Commands::Purge { db: ref db_path, dry_run }) => {
            let db = open_db_or_exit(db_path);
            match db.purge(dry_run) {
                Ok(report) => print_json(&report),
                Err(e) => {
                    eprintln!("mimir: purge failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Doctor) => {
            println!("Mimir is healthy!");
        }
    }

    // No subcommand given, or `serve` subcommand
    let (
        db_path,
        encryption_key,
        web,
        port,
        web_bind,
        llm_endpoint,
        llm_api_key,
        embedding_endpoint,
        embedding_model,
        llm_model,
        connectors_config,
        web_auth_token,
        transport,
        mcp_token,
        _workspace_token,
    ) = if let Some(Commands::Serve {
        db,
        encryption_key,
        web,
        port,
        web_bind,
        llm_endpoint,
        llm_api_key,
        embedding_endpoint,
        embedding_model,
        llm_model,
        connectors_config,
        web_auth_token,
        transport,
        mcp_token,
        workspace_token,
        ..
    }) = cli.command
    {
        (
            db,
            encryption_key,
            web,
            port,
            web_bind,
            llm_endpoint,
            llm_api_key,
            embedding_endpoint,
            embedding_model,
            llm_model,
            connectors_config,
            web_auth_token,
            transport,
            mcp_token,
            workspace_token,
        )
    } else if cli.command.is_none() {
        (
            cli.db.unwrap_or_else(default_db_path),
            cli.encryption_key,
            cli.web,
            cli.port,
            cli.web_bind,
            cli.llm_endpoint,
            cli.llm_api_key,
            cli.embedding_endpoint,
            cli.embedding_model,
            cli.llm_model,
            cli.connectors_config,
            cli.web_auth_token,
            cli.transport,
            cli.mcp_token,
            cli.workspace_token,
        )
    } else {
        return;
    };

    check_legacy_db(&db_path);
    let mut db = open_db_or_exit(&db_path);
    if let Err(e) = db.with_encryption(encryption_key) {
        eprintln!("mimir: encryption setup failed: {}", e);
        std::process::exit(1);
    }
    if let Err(e) = db.with_llm(
        llm_endpoint,
        llm_api_key,
        llm_model,
        embedding_endpoint,
        embedding_model,
    ) {
        eprintln!("mimir: LLM setup failed: {}", e);
        std::process::exit(1);
    }
    db.with_connectors(connectors_config);

    let web_running = if web {
        let db_clone = db.clone();
        let web_bind_clone = web_bind.clone();
        Some(tokio::spawn(async move {
            web::run_server(db_clone, port, web_bind_clone, web_auth_token).await
        }))
    } else {
        None
    };

    match transport.as_str() {
        "sse" => transport::sse::run_server(db, port, mcp_token),
        "http" => transport::http::run_server(db, port, mcp_token),
        _ => mcp::run_server(db),
    };

    if let Some(handle) = web_running {
        handle.abort();
    }
}
