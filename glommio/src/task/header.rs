// Unless explicitly stated otherwise all files in this repository are licensed
// under the MIT/Apache-2.0 License, at your convenience
//
// This product includes software developed at Datadog (https://www.datadoghq.com/). Copyright 2020 Datadog, Inc.
//
use core::{fmt, task::Waker};

use crate::task::{raw::TaskVTable, state::*, utils::abort_on_panic};

use crate::sys::SleepNotifier;
use std::sync::Arc;

use std::sync::atomic::{AtomicU16, Ordering};

/// The header of a task.
///
/// This header is stored right at the beginning of every heap-allocated task.
pub(crate) struct Header {
    /// ID of the executor to which task belongs to or in another words by which
    /// task was spawned by
    pub(crate) notifier: Arc<SleepNotifier>,

    /// Current state of the task.
    pub(crate) state: u8,

    /// Current reference count of the task.
    pub(crate) references: AtomicU16,

    /// The task that is blocked on the `JoinHandle`.
    ///
    /// This waker needs to be woken up once the task completes or is closed.
    pub(crate) awaiter: Option<Waker>,

    /// The virtual table.
    ///
    /// In addition to the actual waker virtual table, it also contains pointers
    /// to several other methods necessary for bookkeeping the
    /// heap-allocated task.
    pub(crate) vtable: &'static TaskVTable,
}

impl Header {
    /// Cancels the task.
    ///
    /// This method will mark the task as closed, but it won't reschedule the
    /// task or drop its future.
    pub(crate) fn cancel(&mut self) {
        // If the task has been completed or closed, it can't be canceled.
        if self.state & (COMPLETED | CLOSED) != 0 {
            return;
        }

        // Mark the task as closed.
        self.state |= CLOSED;
    }

    /// Notifies the awaiter blocked on this task.
    ///
    /// If the awaiter is the same as the current waker, it will not be
    /// notified.
    #[inline]
    pub(crate) fn notify(&mut self, current: Option<&Waker>) {
        // Take the waker out.
        let waker = self.awaiter.take();

        if let Some(w) = waker {
            // We need a safeguard against panics because waking can panic.
            abort_on_panic(|| match current {
                None => w.wake(),
                Some(c) if !w.will_wake(c) => w.wake(),
                Some(_) => {}
            });
        }
    }

    /// Registers a new awaiter blocked on this task.
    ///
    /// This method is called when `JoinHandle` is polled and the task has not
    /// completed.
    #[inline]
    pub(crate) fn register(&mut self, waker: &Waker) {
        // Put the waker into the awaiter field.
        abort_on_panic(|| self.awaiter = Some(waker.clone()));
    }
}

impl fmt::Debug for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.state;
        let refcount = self.references.load(Ordering::Relaxed);

        f.debug_struct("Header")
            .field("thread_id", &self.notifier.id())
            .field("scheduled", &(state & SCHEDULED != 0))
            .field("running", &(state & RUNNING != 0))
            .field("completed", &(state & COMPLETED != 0))
            .field("closed", &(state & CLOSED != 0))
            .field("handle", &(state & HANDLE != 0))
            .field("refcount", &refcount)
            .finish()
    }
}
