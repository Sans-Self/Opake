use super::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

// A deterministic clock whose only advance comes from the injected sleep, so
// the retry window closes exactly when the scheduled delays sum past it — no
// wall-clock dependence, no flakiness.
fn clock_and_sleep() -> (impl Fn() -> u64, SleepFn) {
    let micros = Rc::new(Cell::new(0u64));
    let read = {
        let micros = Rc::clone(&micros);
        move || micros.get()
    };
    let sleep: SleepFn = Box::new(move |d| {
        micros.set(micros.get() + (d.as_millis() as u64) * 1_000);
        Box::pin(async {})
    });
    (read, sleep)
}

const WORKSPACE: &str = "at://did:plc:me/at.opake.keyring/genesis";

fn not_indexed() -> Error {
    Error::WorkspaceNotIndexed {
        workspace_id: WORKSPACE.into(),
    }
}

// spec:indexer-consistency § Dependent operations tolerate the visibility gap
#[test]
fn visibility_gap_covers_indexer_lag_only() {
    assert!(is_visibility_gap(&not_indexed()));
    assert!(is_visibility_gap(&Error::NotFound(
        "no indexed keyring".into()
    )));
    // An individual record the operation awaits is not in the index yet — the
    // uncoded not-found keeps its lag semantics.
    assert!(is_visibility_gap(&Error::Indexer {
        status: 404,
        message: "absent".into()
    }));

    // The definitive denial: a head was consulted and the caller is not in it.
    // Absorbing it would trade a truthful error for a window of latency.
    assert!(!is_visibility_gap(&Error::NotWorkspaceMember {
        workspace_id: WORKSPACE.into()
    }));

    // Genuine failures are surfaced, never absorbed as lag.
    assert!(!is_visibility_gap(&Error::Indexer {
        status: 403,
        message: "forbidden".into()
    }));
    assert!(!is_visibility_gap(&Error::Indexer {
        status: 500,
        message: "boom".into()
    }));
    assert!(!is_visibility_gap(&Error::Auth("bad token".into())));
    assert!(!is_visibility_gap(&Error::ChainAuthorityViolation {
        uri: "at://x".into(),
        author_did: "did:x".into()
    }));
}

#[test]
fn schedule_backs_off_then_exhausts_at_the_window() {
    let mut schedule = VisibilityRetry::new();
    // Feed the elapsed clock forward exactly as the delays accumulate.
    let mut elapsed = 0u64;
    let mut delays = Vec::new();
    while let Some(delay) = schedule.next_delay(elapsed) {
        let ms = delay.as_millis() as u64;
        delays.push(ms);
        elapsed += ms;
        assert!(delays.len() < 20, "schedule must terminate");
    }
    // Exponential from the initial delay, capped at the per-sleep ceiling,
    // with the final delay trimmed so the total never exceeds the window.
    assert_eq!(&delays[..6], &[250, 500, 1000, 2000, 4000, 4000]);
    assert_eq!(delays.iter().sum::<u64>(), MAX_WINDOW_MS);
}

// spec:indexer-consistency § Dependent operations tolerate the visibility gap
#[tokio::test]
async fn retries_workspace_not_indexed_then_succeeds() {
    // not-indexed → not-indexed → 200: the creator's genesis lands mid-window,
    // the two transient answers are absorbed, the value comes back.
    let responses: Rc<RefCell<Vec<Result<u32, Error>>>> = Rc::new(RefCell::new(vec![
        Err(not_indexed()),
        Err(not_indexed()),
        Ok(42),
    ]));
    let calls = Rc::new(Cell::new(0u32));

    let (now, mut sleep) = clock_and_sleep();
    let op = {
        let responses = Rc::clone(&responses);
        let calls = Rc::clone(&calls);
        move || {
            let responses = Rc::clone(&responses);
            let calls = Rc::clone(&calls);
            async move {
                calls.set(calls.get() + 1);
                responses.borrow_mut().remove(0)
            }
        }
    };

    let result = retry_visibility(
        &format!("resolving keyring chain head for {WORKSPACE}"),
        now,
        &mut sleep,
        op,
    )
    .await;
    assert_eq!(result.unwrap(), 42);
    assert_eq!(calls.get(), 3);
}

// spec:indexer-consistency § Dependent operations tolerate the visibility gap
#[tokio::test]
async fn exhausted_window_yields_timeout_naming_the_workspace() {
    // Every attempt answers not-indexed: the window closes and the caller gets
    // a VisibilityTimeout naming the workspace it waited on — a wrong-kind id
    // burns the window the same way, and the name is what makes that visible
    // instead of it reading as a slow pipeline.
    let (now, mut sleep) = clock_and_sleep();
    let op = || async { Err::<u32, _>(not_indexed()) };

    let result = retry_visibility(
        &format!("resolving keyring chain head for {WORKSPACE}"),
        now,
        &mut sleep,
        op,
    )
    .await;
    match result {
        Err(error @ Error::VisibilityTimeout { .. }) => {
            assert!(
                error.to_string().contains(WORKSPACE),
                "the timeout must name the workspace it waited on: {error}"
            );
            let Error::VisibilityTimeout { waited_ms, .. } = error else {
                unreachable!()
            };
            assert!(waited_ms >= MAX_WINDOW_MS - MAX_DELAY_MS);
        }
        other => panic!("expected VisibilityTimeout, got {other:?}"),
    }
}

// spec:indexer-consistency § Dependent operations tolerate the visibility gap
#[tokio::test]
async fn authorization_denial_is_not_retried() {
    // The indexer only denies after reading an indexed head, so the denial is
    // definitive: one attempt, surfaced as-is, window untouched.
    let calls = Rc::new(Cell::new(0u32));
    let (now, mut sleep) = clock_and_sleep();
    let op = {
        let calls = Rc::clone(&calls);
        move || {
            let calls = Rc::clone(&calls);
            async move {
                calls.set(calls.get() + 1);
                Err::<u32, _>(Error::NotWorkspaceMember {
                    workspace_id: WORKSPACE.into(),
                })
            }
        }
    };

    let result = retry_visibility("resolving chain head", &now, &mut sleep, op).await;
    match result {
        Err(Error::NotWorkspaceMember { workspace_id }) => assert_eq!(workspace_id, WORKSPACE),
        other => panic!("expected NotWorkspaceMember, got {other:?}"),
    }
    assert_eq!(calls.get(), 1);
    assert_eq!(now(), 0, "the denial must not consume the retry window");
}

#[tokio::test]
async fn non_gap_error_is_surfaced_immediately() {
    // A real failure is not retried — one attempt, error passed straight up.
    let calls = Rc::new(Cell::new(0u32));
    let (now, mut sleep) = clock_and_sleep();
    let op = {
        let calls = Rc::clone(&calls);
        move || {
            let calls = Rc::clone(&calls);
            async move {
                calls.set(calls.get() + 1);
                Err::<u32, _>(Error::ChainAuthorityViolation {
                    uri: "at://x".into(),
                    author_did: "did:x".into(),
                })
            }
        }
    };

    let result = retry_visibility("resolving chain head", now, &mut sleep, op).await;
    assert!(matches!(result, Err(Error::ChainAuthorityViolation { .. })));
    assert_eq!(calls.get(), 1);
}
