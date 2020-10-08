//! Mula provides a way of having several requests for a given computation
//! be serviced without duplicating the actual computation.
//!
//! Imagine you have a REST API with an endpoint that causes an expensive
//! computation. If that endpoint receives more requests while the first one
//! is being computed it would be nice if you could just let those new
//! requests wait in line for the computation to end and give them all the
//! same result.
//!
//! That is what Mula allows you to do.
//!
//! # Example
//!
//! The following example will only run the computation closure twice,
//! once for each of the distinct inputs. The two subscriptions for "burro"
//! will both be serviced by the same computation.
//!
//! ```rust
//! use mula::Mula;
//!
//! fn main() {
//!     let mula = Mula::new(|input: &str| {
//!         std::thread::sleep(std::time::Duration::from_secs(2));
//!         input.to_uppercase()
//!     });
//!
//!     let m = mula.clone();
//!     let thread1 = std::thread::spawn(move || {
//!         let upper = Mula::subscribe_to(m, "mula");
//!         assert_eq!(upper, "MULA".to_string());
//!     });
//!
//!     let m = mula.clone();
//!     let thread2 = std::thread::spawn(move || {
//!         let upper = Mula::subscribe_to(m, "burro");
//!         assert_eq!(upper, "BURRO".to_string());
//!     });
//!
//!     let m = mula.clone();
//!     let thread3 = std::thread::spawn(move || {
//!         let upper = Mula::subscribe_to(m, "burro");
//!         assert_eq!(upper, "BURRO".to_string());
//!     });
//!
//!     thread1.join();
//!     thread2.join();
//!     thread3.join();
//! }
//! ```


#![allow(dead_code)]
use parking_lot::Mutex;
use spmc;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Weak};

/// State tracker that allows for sharing of a specific computation.
pub struct Mula<T, O, F>
where
    T: 'static + Eq + Hash + Clone + Send,
    O: 'static + Clone + Send,
    F: 'static + Send + Sync + Fn(T) -> O
{
    map: Mutex<HashMap<T, Bus<O>>>,
    work: F,
}

impl<T, O, F> Mula<T, O, F>
where
    T: 'static + Eq + Hash + Clone + Send,
    O: 'static + Clone + Send,
    F: 'static + Send + Sync + Fn(T) -> O
{
    /// Creates a new `Mula` that will own the `work` closure.
    /// The return value is wrapped in an [`std::sync::Arc`] so that
    /// you can safely [`std::sync::Arc::clone()`] it to give threads their own reference
    /// of the state tracker.
    ///
    /// Note that the `work` closure will be called on a separate
    /// thread.
    ///
    /// # Example:
    ///
    /// ```
    /// use mula::Mula;
    ///
    /// let mula = Mula::new(|input: &str| {
    ///     std::thread::sleep(std::time::Duration::from_secs(2));
    ///     input.to_uppercase()
    /// });
    ///
    /// // Both of the following call sites will share the same execution
    /// // of the closure and get the same result.
    /// let m = mula.clone();
    /// std::thread::spawn(move || {
    ///     let upper = Mula::subscribe_to(m, "mula");
    ///     assert_eq!(upper, "MULA".to_string());
    /// });
    ///
    /// let upper = Mula::subscribe_to(mula, "mula");
    /// assert_eq!(upper, "MULA".to_string());
    /// ```
    pub fn new(work: F) -> Arc<Self> {
        Arc::new(Self {
            map: Mutex::new(HashMap::<T, Bus<O>>::new()),
            work
        })
    }

    /// Registers your interest in the output of the computation
    /// defined by the closure provided to [`Mula::new`] and `input`.
    ///
    /// When this function is called, it will:
    ///
    /// - start the computation, if it is not already running for `input`
    /// - block and wait for the computation to finish
    /// - return a clone of the result
    ///
    /// If your `work` closure returns something big, it may be wise
    /// to wrap it on something like an [`std::sync::Arc`], so that it
    /// isn't deep copied for each of the subscribers.
    ///
    /// # Example:
    ///
    /// ```
    /// use mula::Mula;
    ///
    /// let mula = Mula::new(|input: &str| {
    ///     input.to_uppercase()
    /// });
    ///
    /// let upper = Mula::subscribe_to(mula, "mula");
    /// assert_eq!(upper, "MULA".to_string());
    /// ```
    pub fn subscribe_to(mula: Arc<Self>, input: T) -> O {
        // This gets the Bus from the map if it has already been inserted,
        // otherwise calls the work closure and inserts a new Bus on the
        // map and returns it.
        let thread_mula = mula.clone();
        let mut map = mula.map.lock();
        let bus = map.entry(input.clone())
            .or_insert_with(|| {
                std::thread::spawn(move || {
                    let key = input.clone();
                    let result = (thread_mula.work)(input);
                    let mut map = thread_mula.map.lock();
                    let bus = map.get_mut(&key).unwrap();
                    bus.broadcast(result);
                });
                Bus::new()
            });

        // Now that we have the Bus, get a receiver for this thread.
        let receiver = bus.add_receiver();

        // IMPORTANT: need to drop the bus explicitly here, as we are going
        // to block on the recv() call below, and would be effectively holding
        // a lock on the map, leading to a deadlock.
        drop(bus);
        drop(map);

        // Here we wait for the processing we are interested in to finish.
        receiver.recv().unwrap()
    }
}

type Receiver<O> = Arc<spmc::Receiver<O>>;

struct Bus<O: Clone + Send> {
    sender: spmc::Sender<O>,
    receiver: spmc::Receiver<O>,
    weak_receivers: Vec<Weak<spmc::Receiver<O>>>,
}

impl<O: Clone + Send> Bus<O> {
    fn new() -> Self {
        let (sender, receiver) = spmc::channel();
        Bus {
            sender,
            receiver,
            weak_receivers: Vec::<Weak<spmc::Receiver<O>>>::new(),
        }
    }

    fn add_receiver(&mut self) -> Receiver<O> {
        let receiver = Arc::new(self.receiver.clone());
        self.weak_receivers.push(Arc::downgrade(&receiver));
        receiver
    }

    fn broadcast(&mut self, msg: O) {
        let sender = &mut self.sender;
        for receiver in &self.weak_receivers {
            if receiver.strong_count() > 0 {
                sender.send(msg.clone()).unwrap();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn it_works() {
        let work_counter = Arc::new(AtomicUsize::new(0));
        let subscriber_counter = Arc::new(AtomicUsize::new(0));

        let counter = work_counter.clone();
        let mula = Mula::new(move |input: &str| {
            match input {
                "kov" => std::thread::sleep(std::time::Duration::from_secs(2)),
                "kovera" => {
                    let mut rng = rand::thread_rng();
                    std::thread::sleep(std::time::Duration::from_secs(rng.gen_range(0, 10)));
                },
                _ => (),
            }
            counter.fetch_add(1, Ordering::SeqCst);
            input.to_uppercase()
        });

        let m = mula.clone();
        let counter = subscriber_counter.clone();
        let thread1 = std::thread::spawn(move || {
            let upper = Mula::subscribe_to(m, "kov");
            assert_eq!(upper, "KOV".to_string());
            counter.fetch_add(1, Ordering::SeqCst);
        });

        let m = mula.clone();
        let counter = subscriber_counter.clone();
        let thread2 = std::thread::spawn(move || {
            let upper = Mula::subscribe_to(m, "kov");
            assert_eq!(upper, "KOV".to_string());
            counter.fetch_add(1, Ordering::SeqCst);
        });

        let m = mula.clone();
        let counter = subscriber_counter.clone();
        let thread3 = std::thread::spawn(move || {
            let upper = Mula::subscribe_to(m, "kovera");
            assert_eq!(upper, "KOVERA".to_string());
            counter.fetch_add(1, Ordering::SeqCst);
        });

        let m = mula.clone();
        let counter = subscriber_counter.clone();
        let thread4 = std::thread::spawn(move || {
            let upper = Mula::subscribe_to(m, "kovera");
            assert_eq!(upper, "KOVERA".to_string());
            counter.fetch_add(1, Ordering::SeqCst);
        });

        let m = mula.clone();
        let counter = subscriber_counter.clone();
        let thread5 = std::thread::spawn(move || {
            let upper = Mula::subscribe_to(m, "kovid");
            assert_eq!(upper, "KOVID".to_string());
            counter.fetch_add(1, Ordering::SeqCst);
        });

        let result = Mula::subscribe_to(mula, "kov");
        assert_eq!(result, "KOV");
        subscriber_counter.fetch_add(1, Ordering::SeqCst);

        thread1.join().unwrap();
        thread2.join().unwrap();
        thread3.join().unwrap();
        thread4.join().unwrap();
        thread5.join().unwrap();

        // We have 6 call sites, we should see all of them
        // getting a reply.
        assert_eq!(subscriber_counter.load(Ordering::SeqCst), 6);

        // We should be able to share the work for each input,
        // so we expect only a single work() call for each.
        assert_eq!(work_counter.load(Ordering::SeqCst), 3);
    }
}
