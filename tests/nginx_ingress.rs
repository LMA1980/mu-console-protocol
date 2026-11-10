// SPDX-License-Identifier: LGPL-3.0-only WITH LGPL-3.0-linking-exception
//! Integration test validating Nginx HTTP/3 (QUIC / h3) ingress configuration
//! in `nginx/muconsole.conf` (#293–#296).

use std::path::{Path, PathBuf};

fn find_nginx_conf() -> Option<PathBuf> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest_path = Path::new(manifest_dir);
    let candidates = [
        manifest_path.join("../../nginx/muconsole.conf"),
        manifest_path.join("../nginx/muconsole.conf"),
        manifest_path.join("nginx/muconsole.conf"),
        PathBuf::from("../../nginx/muconsole.conf"),
        PathBuf::from("../nginx/muconsole.conf"),
        PathBuf::from("nginx/muconsole.conf"),
    ];

    for path in &candidates {
        if path.is_file() {
            return Some(path.clone());
        }
    }
    None
}

#[test]
fn test_nginx_http3_quic_ingress_directives() {
    let conf_path = match find_nginx_conf() {
        Some(path) => path,
        None => {
            // When running in an isolated submodule standalone CI without parent repo
            eprintln!("nginx/muconsole.conf not present in current directory hierarchy; skipping");
            return;
        }
    };

    let conf = std::fs::read_to_string(&conf_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", conf_path.display(), e));

    // 1. Validate QUIC listen directives
    assert!(
        conf.contains("listen 443 quic reuseport;"),
        "nginx/muconsole.conf must specify 'listen 443 quic reuseport;'"
    );
    assert!(
        conf.contains("listen [::]:443 quic;"),
        "nginx/muconsole.conf must specify 'listen [::]:443 quic;'"
    );

    // 2. Validate HTTP/2 and HTTP/3 directives
    assert!(
        conf.contains("http2 on;"),
        "nginx/muconsole.conf must enable http2 on;"
    );
    assert!(
        conf.contains("http3 on;"),
        "nginx/muconsole.conf must enable http3 on;"
    );
    assert!(
        conf.contains("quic_retry on;"),
        "nginx/muconsole.conf must enable quic_retry on; for address validation"
    );

    // 3. Validate TLSv1.3 for QUIC requirement
    assert!(
        conf.contains("TLSv1.3"),
        "nginx/muconsole.conf must include TLSv1.3 in ssl_protocols for QUIC compliance"
    );

    // 4. Validate Alt-Svc advertisement at server level and in overriding location blocks
    let alt_svc_pattern = "add_header Alt-Svc 'h3=\":443\"; ma=86400' always;";
    assert!(
        conf.contains(alt_svc_pattern),
        "nginx/muconsole.conf must advertise Alt-Svc header: {}",
        alt_svc_pattern
    );

    // Ensure Alt-Svc is defined at least 5 times (server level + 4 location blocks)
    let alt_svc_occurrences = conf.matches(alt_svc_pattern).count();
    assert!(
        alt_svc_occurrences >= 5,
        "Expected Alt-Svc header in server block and 4 overriding location blocks (sign_out, cards/badges, static assets, root), found {} occurrences",
        alt_svc_occurrences
    );

    // 5. Verify specific location blocks contain Alt-Svc
    let sign_out_block = conf
        .split("location = /oauth2/sign_out")
        .nth(1)
        .and_then(|s| s.split('}').next())
        .expect("location = /oauth2/sign_out block must exist");
    assert!(
        sign_out_block.contains(alt_svc_pattern),
        "/oauth2/sign_out location block must preserve Alt-Svc header"
    );

    let cards_block = conf
        .split("location ~ ^/(cards|badges)/")
        .nth(1)
        .and_then(|s| s.split('}').next())
        .expect("location ~ ^/(cards|badges)/ block must exist");
    assert!(
        cards_block.contains(alt_svc_pattern),
        "cards/badges location block must preserve Alt-Svc header"
    );

    let static_assets_block = conf
        .split(r#"location ~* \.(?:wasm|js|css|d\.ts|woff2?|png|svg|ico)$"#)
        .nth(1)
        .and_then(|s| s.split('}').next())
        .expect("static assets location block must exist");
    assert!(
        static_assets_block.contains(alt_svc_pattern),
        "static assets location block must preserve Alt-Svc header"
    );

    let root_block = conf
        .split("location / {")
        .nth(1)
        .and_then(|s| s.split('}').next())
        .expect("location / block must exist");
    assert!(
        root_block.contains(alt_svc_pattern),
        "root location block must preserve Alt-Svc header"
    );
}
