//! Sonde HTTP minimale.
//!
//! Cloud Run n'accepte un conteneur de type *service* que s'il écoute sur le
//! port fourni par la variable `PORT` : sans cela, la révision est marquée en
//! échec au démarrage même si le travail réel est du pull Pub/Sub.
//!
//! On répond donc `200 ok` à toute requête, sans framework HTTP (pas de
//! dépendance supplémentaire, quelques Ko de binaire en plus).

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 3\r\nConnection: close\r\n\r\nok\n";

/// Sert la sonde jusqu'à l'annulation du token (SIGTERM / SIGINT).
pub async fn serve(port: u16, shutdown: CancellationToken) {
    let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!(error = %e, port, "Cannot bind health endpoint");
            return;
        }
    };

    tracing::info!(port, "Health endpoint listening");

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("Health endpoint stopped");
                return;
            }

            accepted = listener.accept() => match accepted {
                Ok((mut stream, _peer)) => {
                    tokio::spawn(async move {
                        // On consomme la requête avant de répondre, sinon le
                        // client reçoit un RST au lieu de la réponse.
                        let mut buf = [0u8; 1024];
                        let _ = tokio::time::timeout(
                            Duration::from_secs(2),
                            stream.read(&mut buf),
                        ).await;
                        let _ = stream.write_all(RESPONSE).await;
                        let _ = stream.shutdown().await;
                    });
                }
                Err(e) => tracing::warn!(error = %e, "Health accept failed"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La sonde répond 200 puis s'arrête proprement à l'annulation du token :
    /// c'est ce que vérifie Cloud Run au démarrage de la révision.
    #[tokio::test]
    async fn responds_200_and_shuts_down() {
        let shutdown = CancellationToken::new();
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener); // On libère le port choisi par l'OS pour le réutiliser.

        let task = tokio::spawn(serve(port, shutdown.clone()));

        // Laisse le temps au bind ; la boucle d'accept est immédiate ensuite.
        let mut stream = None;
        for _ in 0..50 {
            match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
        let mut stream = stream.expect("health endpoint injoignable");

        stream
            .write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "réponse : {response}"
        );
        assert!(response.ends_with("ok\n"), "réponse : {response}");

        shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("la sonde ne s'est pas arrêtée")
            .unwrap();
    }
}
