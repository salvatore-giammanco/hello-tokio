use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam::channel;
use futures::task;
use futures::task::ArcWake;

// Used to track the current mini-tokio instance so that the `spawn` function is able to schedule
// spawned tasks.
thread_local! {
    static CURRENT: RefCell<Option<channel::Sender<Arc<Task>>>> =
        RefCell::new(None);
}

fn main() {
    let mut mini_tokio = MiniTokio::new();

    // Spawn the root task. All other tasks are spawned from the context of this root task,
    // No work happens until `mini_tokio.run()` is called.
    mini_tokio.spawn(async {
        spawn(async {
            delay(Duration::from_millis(100)).await;
            print!("world");
        });
        spawn(async {
            print!("Hello ");
        });

        delay(Duration::from_millis(200)).await;
        std::process::exit(0);
    });

    mini_tokio.run();
}

async fn delay(dur: Duration) {
    struct Delay {
        when: Instant,
        // This is Some when we have spawned a thread, and None otherwise.
        waker: Option<Arc<Mutex<Waker>>>,
    }
    impl Future for Delay {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            // If this is the first time the future is called, spawn the time thread,
            // otherwise ensure the stored `Waker` matches the current task's waker.
            if let Some(waker) = &self.waker {
                let mut waker = waker.lock().unwrap();

                if !waker.will_wake(cx.waker()) {
                    *waker = cx.waker().clone();
                }
            } else {
                let when = self.when;
                let waker = Arc::new(Mutex::new(cx.waker().clone()));
                self.waker = Some(waker.clone());

                // This is the first time `poll` is called, spawn the timer thread.
                thread::spawn(move || {
                    let now = Instant::now();
                    if now < when {
                        thread::sleep(when - now);
                    }
                    let waker = waker.lock().unwrap();
                    waker.wake_by_ref();
                });
            }

            if Instant::now() >= self.when {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        }
    }

    // Create an instance of our `Delay` future.
    let future = Delay {
        when: Instant::now() + dur,
        waker: None,
    };

    // Wait for the duration to complete.
    future.await;
}

struct Task {
    future: Mutex<Pin<Box<dyn Future<Output = ()> + Send>>>,
    executor: channel::Sender<Arc<Task>>,
}

impl Task {
    fn schedule(self: &Arc<Self>) {
        let _ = self.executor.send(self.clone());
    }

    fn poll(self: Arc<Self>) {
        // Create a waker from the `Task` Instance. This uses the `ArcWake` impl from above.
        let waker = task::waker(self.clone());
        let mut cx = Context::from_waker(&waker);

        // No other thread ever tries to look the future
        let mut future = self.future.try_lock().unwrap();

        // Poll the future
        let _ = future.as_mut().poll(&mut cx);
    }

    fn spawn<F>(future: F, sender: &channel::Sender<Arc<Task>>)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let task = Arc::new(Task {
            future: Mutex::new(Box::pin(future)),
            executor: sender.clone(),
        });
        let _ = sender.send(task);
    }
}

impl ArcWake for Task {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.schedule();
    }
}

struct MiniTokio {
    scheduled: channel::Receiver<Arc<Task>>,
    sender: channel::Sender<Arc<Task>>,
}

impl MiniTokio {
    fn new() -> MiniTokio {
        let (sender, scheduled) = channel::unbounded();
        MiniTokio { scheduled, sender }
    }

    /// Spawn a future onto the mini-tokio instance.
    /// The given future i wrapped with the `Task` harness and pushed into the `scheduled` queue.
    /// The future will be executed when `run` is called.
    fn spawn<F>(&mut self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        Task::spawn(future, &self.sender);
    }

    fn run(&mut self) {
        CURRENT.with(|cell| {
            *cell.borrow_mut() = Some(self.sender.clone());
        });

        while let Ok(task) = self.scheduled.recv() {
            task.poll();
        }
    }
}

// Equivalent to `tokio::spawn`.
// When entering the mini-tokio executor, the `CURRENT` thread-local is set to point to that
// executor's channel's Send half. Then, spawning requires creating the `Task` harness for the
// given `future` and pushing it into the scheduled queue.
pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    CURRENT.with(|cell| {
        let borrow = cell.borrow();
        let sender = borrow.as_ref().unwrap();
        Task::spawn(future, sender);
    });
}
