use open_metrics_client::counter::Counter;
use open_metrics_client::encoding::text::encode;
use open_metrics_client::registry::{Descriptor, Registry};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use tide::{Middleware, Next, Request, Result};

#[async_std::main]
async fn main() -> std::result::Result<(), std::io::Error> {
    let mut registry = Registry::new();
    let counter = Counter::new();
    registry.register(
        Descriptor::new("counter", "my counter", "my_counter"),
        counter.clone(),
    );
    let middleware = MetricsMiddleware {
        num_requests: counter,
    };

    tide::log::start();
    let mut app = tide::with_state(State {
        registry: Arc::new(Mutex::new(registry)),
    });
    app.with(middleware);
    app.at("/").get(|_| async { Ok("Hello, world!") });
    app.at("/metrics")
        .get(|req: tide::Request<State>| async move {
            let mut encoded = Vec::new();
            encode::<_, _>(
                &mut encoded,
                &req.state().registry.lock().unwrap(),
            )
            .unwrap();
            Ok(String::from_utf8(encoded).unwrap())
        });
    app.listen("127.0.0.1:8080").await?;
    Ok(())
}

struct State {
    registry: Arc<Mutex<Registry<Counter<AtomicU64>>>>,
}

impl Clone for State {
    fn clone(&self) -> Self {
        State {
            registry: self.registry.clone(),
        }
    }
}

#[derive(Default)]
struct MetricsMiddleware {
    num_requests: Counter<AtomicU64>,
}

#[tide::utils::async_trait]
impl<State: Clone + Send + Sync + 'static> Middleware<State> for MetricsMiddleware {
    async fn handle(&self, req: Request<State>, next: Next<'_, State>) -> Result {
        let _count = self.num_requests.inc();

        let res = next.run(req).await;
        Ok(res)
    }
}
