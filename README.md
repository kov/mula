# mula

Mula provides a way of having several requests for a given computation
be serviced without duplicating the actual computation.

Imagine you have a REST API with an endpoint that causes an expensive
computation. If that endpoint receives more requests while the first one
is being computed it would be nice if you could just let those new
requests wait in line for the computation to end and give them all the
same result.

That is what Mula allows you to do.

# Example

The following example will only run the computation closure twice,
once for each of the distinct inputs. The two subscriptions for "burro"
will both be serviced by the same computation.

```rust
use mula::mula;

#[mula]
fn delayed_uppercase(input: &'static str) -> String {
    std::thread::sleep(std::time::Duration::from_secs(2));
    input.to_uppercase()
}

let thread1 = std::thread::spawn(move || {
    let upper = delayed_uppercase("mula");
    assert_eq!(upper, "MULA".to_string());
});

let thread2 = std::thread::spawn(move || {
    let upper = delayed_uppercase("burro");
    assert_eq!(upper, "BURRO".to_string());
});

let thread3 = std::thread::spawn(move || {
    let upper = delayed_uppercase("burro");
    assert_eq!(upper, "BURRO".to_string());
});

thread1.join();
thread2.join();
thread3.join();
```

License: MIT
