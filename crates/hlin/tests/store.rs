//! The store's behaviour, checked against both implementations.
//!
//! One suite, two stores. That is the whole design: the in-memory store is only
//! useful for as long as it agrees with the one production uses, and the way to
//! know it still agrees is to run the same assertions against both.
//!
//! The Postgres case used to be `#[ignore]`d, which meant it ran when somebody
//! remembered — and `angreal test all` did not pass `--ignored`, so nobody did.
//! HLIN-A-0006 makes Postgres the only production backend, and the debounce
//! HLIN-A-0002 depends on lives in one `ON CONFLICT … CASE` statement there. A
//! divergence between the two stores would break contract enforcement in
//! production while every test passed.
//!
//! So it decides for itself instead. If a database answers it runs; if not it
//! says so, loudly, and returns. `HLIN_REQUIRE_DATABASE=1` turns that skip into
//! a failure, which is what CI sets: a developer without a database should not
//! be blocked, and a pipeline without one should not be quietly green.

use std::time::Duration;

use hlin::store::{MemoryStore, PostgresStore};

mod store_suite;

/// Where to look for a database.
fn database_url() -> String {
    std::env::var("HLIN_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://hlin:hlin@localhost:55432/hlin".to_string())
}

/// Whether a pipeline has declared that a database must be present.
fn database_required() -> bool {
    std::env::var("HLIN_REQUIRE_DATABASE").is_ok_and(|value| value != "0" && !value.is_empty())
}

#[tokio::test]
async fn the_in_memory_store_behaves() {
    store_suite::run_all(&MemoryStore::new()).await;
}

/// The same suite against a real database.
///
/// This is the case that matters. The other one is a convenience.
#[tokio::test]
async fn the_postgres_store_behaves() {
    let url = database_url();

    // A short timeout rather than the driver's default, because the common case
    // for a developer is that nothing is listening at all, and waiting thirty
    // seconds to be told so on every test run is its own reason not to run it.
    let connected =
        tokio::time::timeout(Duration::from_secs(5), PostgresStore::connect(&url)).await;

    let store = match connected {
        Ok(Ok(store)) => store,
        Ok(Err(error)) => return unavailable(&url, &error.to_string()),
        Err(_) => return unavailable(&url, "connecting timed out"),
    };

    store_suite::run_all(&store).await;
}

/// No database. Say so in a way somebody will notice, and fail where a pipeline
/// has said one must be there.
fn unavailable(url: &str, reason: &str) {
    if database_required() {
        panic!(
            "HLIN_REQUIRE_DATABASE is set but no database answered at {url}: {reason}\n\
             This is the only production backend (HLIN-A-0006); a green run without \
             it proves nothing about the store."
        );
    }

    eprintln!(
        "\n  SKIPPED: the_postgres_store_behaves — no database at {url} ({reason})\n\
         \x20 Postgres is the only production backend and this is the suite that \
         checks it.\n\
         \x20 Start one with `angreal db up`, or set HLIN_REQUIRE_DATABASE=1 to make \
         this a failure.\n"
    );
}
