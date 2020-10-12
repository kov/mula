use std::sync::atomic::{AtomicUsize, Ordering};
use lazy_static::lazy_static;

use mula::mula;

lazy_static! {
    static ref WORK_COUNTER: AtomicUsize = AtomicUsize::new(0);
    static ref SUBS_COUNTER: AtomicUsize = AtomicUsize::new(0);
}

#[test]
fn test_basic() {
    #[mula]
    fn mula_fn(input: &'static str) -> String {
        std::thread::sleep(std::time::Duration::from_secs(1));
        WORK_COUNTER.fetch_add(1, Ordering::SeqCst);
        input.to_uppercase()
    }

    let thread1 = std::thread::spawn(|| {
        assert_eq!(mula_fn("kov"), "KOV".to_string());
        SUBS_COUNTER.fetch_add(1, Ordering::SeqCst);

    });

    let thread2 = std::thread::spawn(|| {
        assert_eq!(mula_fn("kov"), "KOV".to_string());
        SUBS_COUNTER.fetch_add(1, Ordering::SeqCst);

    });

    assert_eq!(mula_fn("kov"), "KOV".to_string());
    SUBS_COUNTER.fetch_add(1, Ordering::SeqCst);

    thread1.join().unwrap();
    thread2.join().unwrap();

    // We should be able to share the work for each input,
    // so we expect only a single work() call for each.
    assert_eq!(WORK_COUNTER.load(Ordering::SeqCst), 1);

    // We should be have gotten the response for all the
    // subscribers.
    assert_eq!(SUBS_COUNTER.load(Ordering::SeqCst), 3);
}
