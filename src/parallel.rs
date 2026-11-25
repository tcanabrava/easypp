use std::sync::Mutex;

pub fn for_each<T: Send>(
    items: impl ExactSizeIterator<Item = T> + Send,
    jobs: usize,
    work: impl Fn(T) + Sync,
) {
    for_each_while(items, jobs, || true, work);
}

/// Stops taking new items when the predicate becomes false. In-flight work finishes.
pub fn for_each_while<T: Send>(
    items: impl ExactSizeIterator<Item = T> + Send,
    jobs: usize,
    keep_going: impl Fn() -> bool + Sync,
    work: impl Fn(T) + Sync,
) {
    let workers = jobs.max(1).min(items.len());
    let queue = Mutex::new(items);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while keep_going() {
                    let Some(item) = queue.lock().unwrap().next() else {
                        break;
                    };
                    work(item);
                }
            });
        }
    });
}
