use std::{future::Future, panic::catch_unwind, sync::LazyLock, thread};

use smol::{block_on, future, Executor, Task};

/// TODO: make description
pub fn spawn<T: Send + 'static>(future: impl Future<Output = T> + Send + 'static) -> Task<T> {
    static GLOBAL: LazyLock<Executor<'_>> = LazyLock::new(|| {
        for n in 1..=4 {
            thread::Builder::new()
                .name(format!("smol-{n}"))
                .spawn(|| loop {
                    catch_unwind(|| block_on(GLOBAL.run(future::pending::<()>()))).ok();
                })
                .expect("cannot spawn executor thread");
        }

        Executor::new()
    });

    GLOBAL.spawn(future)
}
