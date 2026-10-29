use std::{
    collections::VecDeque,
    num::NonZero,
    pin::Pin,
    sync::Arc,
    task::{
        Context,
        Poll,
        Waker,
        ready,
    },
};

use bytes::Buf;
use mrrp_core::{
    buf::{
        SampleBuf,
        SampleBufMut,
        TryAdvanceError,
    },
    signal::{
        AsyncReadSamples,
        AsyncWriteSamples,
        Remaining,
        StreamLength,
    },
};
use parking_lot::{
    RwLock,
    RwLockReadGuard,
    RwLockWriteGuard,
};
use pin_project_lite::pin_project;
use tokio::task::coop;

/// The channel is closed and can't ever become open again.
#[derive(Debug, thiserror::Error)]
#[error("The pipe has no sender left")]
pub struct NoSenders;

/// There are no receivers that could read this data at the moment.
#[derive(Debug, thiserror::Error)]
#[error("The pipe has no receivers left")]
pub struct NoReceivers;

/// There are no subscrribers left that could create a new receiver.
#[derive(Debug, thiserror::Error)]
#[error("The pipe has no subscribers left")]
pub struct NoSubscribers;

/// Creates a broadcast pipe
pub fn pipe<T>(buffer_size: NonZero<usize>) -> (Sender<T>, Subscriber<T>) {
    let shared = Arc::new(Shared {
        state: RwLock::new(State {
            buffer: VecDeque::with_capacity(buffer_size.get()),
            head_pos: 0,
            receiver_slots: vec![],
            sender_slot: Some(Slot { waker: None }),
            receiver_count: 0,
            subscriber_count: 1,
            sender_count: 1,
        }),
    });

    (
        Sender {
            shared: shared.clone(),
        },
        Subscriber { shared },
    )
}

#[derive(Debug)]
struct Shared<T> {
    state: RwLock<State<T>>,
}

#[derive(Debug)]
struct State<T> {
    buffer: VecDeque<T>,
    head_pos: usize,

    receiver_slots: Vec<Option<Slot>>,
    sender_slot: Option<Slot>,

    receiver_count: usize,
    subscriber_count: usize,
    sender_count: usize,
}

impl<T> State<T> {
    fn wake_all_receivers(&mut self) {
        // todo: do we need to clear the waker?
        self.receiver_slots
            .iter()
            .flatten()
            .flat_map(|slot| &slot.waker)
            .for_each(|waker| waker.wake_by_ref());
    }

    fn subscribe(&mut self) -> usize {
        // increase receiver count
        self.receiver_count += 1;

        // notify sender that there are receivers
        if let Some(slot) = &self.sender_slot
            && let Some(waker) = &slot.waker
        {
            waker.wake_by_ref();
        }

        // insert a receiver slot
        for (i, slot) in self.receiver_slots.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(Slot { waker: None });
                return i;
            }
        }

        let i = self.receiver_slots.len();
        self.receiver_slots.push(Some(Slot { waker: None }));
        i
    }
}

#[derive(Debug)]
struct Slot {
    waker: Option<Waker>,
}

pin_project! {
    #[derive(Debug)]
    pub struct Receiver<T> {
        inner: ReceiverInner<T>
    }
}

impl<T> Receiver<T> {
    pub fn poll_receive<'a>(
        self: Pin<&'a mut Self>,
        cx: &mut Context,
    ) -> Poll<Result<ReceiveGuard<'a, T>, NoSenders>> {
        let this = self.project();

        // we do this in 2 steps
        //
        // 1: check if there's data and if so return it. this only needs a
        // read guard 2: register waker. this needs a write
        // guard
        //
        // between 1 and 2 we don't hold a lock, so we need to rerun the
        // checks in 2. but 1 can be run by all readers in
        // parallel. we hope this is more efficient (benchmark
        // lol!)

        // first we try to read through a read guard
        let new_waker = {
            let state_guard = this.inner.shared.state.read();

            if this.inner.read_pos < state_guard.head_pos {
                // there's data :)
                return Poll::Ready(Ok(ReceiveGuard {
                    state_guard,
                    read_pos: &mut this.inner.read_pos,
                }));
            }

            // check if there's still a sender
            if state_guard.sender_count == 0 {
                return Poll::Ready(Err(NoSenders));
            }

            // check if we need to insert our waker
            let slot = state_guard.receiver_slots[this.inner.slot]
                .as_ref()
                .unwrap();

            let new_waker = cx.waker();

            if let Some(waker) = &slot.waker
                && new_waker.will_wake(waker)
            {
                None
            }
            else {
                Some(new_waker.clone())
            }
        };

        // at this point we don't hold the lock, so stuff could be written,
        // or the sender drop

        if let Some(waker) = new_waker {
            // we'll need a write guard to insert our waker

            let mut state_guard = this.inner.shared.state.write();

            // but in the meantime some data might have been written to the
            // ring buffer
            if this.inner.read_pos < state_guard.head_pos {
                return Poll::Ready(Ok(ReceiveGuard {
                    state_guard: RwLockWriteGuard::downgrade(state_guard),
                    read_pos: &mut this.inner.read_pos,
                }));
            }

            // check if there's still a sender (again)
            if state_guard.sender_count == 0 {
                return Poll::Ready(Err(NoSenders));
            }

            let slot = state_guard.receiver_slots[this.inner.slot]
                .as_mut()
                .unwrap();

            slot.waker = Some(waker);
        }

        Poll::Pending
    }

    pub fn subscriber(&self) -> Subscriber<T> {
        Subscriber {
            shared: self.inner.shared.clone(),
        }
    }

    pub fn is_closed(&self) -> bool {
        let state_guard = self.inner.shared.state.read();
        state_guard.sender_count == 0
    }
}

impl<T> AsyncReadSamples for Receiver<T>
where
    T: Clone,
{
    type Sample = T;
    type Error = !;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut mrrp_core::signal::ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        match self.poll_receive(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(NoSenders)) => {
                // on EOF we just don't write anything into the buffer
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Ok(guard)) => {
                let n = buffer.remaining_mut();
                buffer.put(guard.take(n));
                Poll::Ready(Ok(()))
            }
        }
    }
}

impl<T> StreamLength for Receiver<T> {
    fn remaining(&self) -> Remaining {
        Remaining::Unknown
    }
}

#[derive(Debug)]
struct ReceiverInner<T> {
    shared: Arc<Shared<T>>,
    slot: usize,
    read_pos: usize,
}

impl<T> Clone for ReceiverInner<T> {
    fn clone(&self) -> Self {
        let mut state_guard = self.shared.state.write();
        let slot = state_guard.subscribe();
        Self {
            shared: self.shared.clone(),
            slot,
            read_pos: self.read_pos,
        }
    }
}

impl<T> Drop for ReceiverInner<T> {
    fn drop(&mut self) {
        let mut state_guard = self.shared.state.write();
        state_guard.receiver_count -= 1;
        state_guard.receiver_slots[self.slot] = None;
    }
}

#[derive(Debug)]
pub struct ReceiveGuard<'a, T> {
    state_guard: RwLockReadGuard<'a, State<T>>,
    read_pos: &'a mut usize,
}

impl<'a, T> ReceiveGuard<'a, T> {
    #[allow(dead_code)]
    pub fn num_dropped(&self) -> usize {
        (self.state_guard.head_pos - *self.read_pos).saturating_sub(self.state_guard.buffer.len())
    }
}

impl<'a, T> SampleBuf<T> for ReceiveGuard<'a, T> {
    fn try_advance(&mut self, amount: usize) -> Result<(), TryAdvanceError> {
        let read_pos = *self.read_pos + amount;

        if read_pos <= self.state_guard.head_pos {
            *self.read_pos = read_pos;
            Ok(())
        }
        else {
            Err(TryAdvanceError {
                requested: amount,
                available: SampleBuf::remaining(self),
            })
        }
    }

    fn remaining(&self) -> usize {
        self.state_guard.head_pos - *self.read_pos
    }

    fn chunk(&self) -> &[T] {
        let mut start_index = self
            .state_guard
            .buffer
            .len()
            .saturating_sub(self.state_guard.head_pos - *self.read_pos);

        let (tail, head) = self.state_guard.buffer.as_slices();

        let chunk = if start_index < tail.len() {
            tail
        }
        else {
            start_index -= tail.len();
            head
        };

        &chunk[start_index..]
    }
}

// these can be delegated to the [`SampleBuf::<u8>`][SampleBuf] implementation.
impl<'a> Buf for ReceiveGuard<'a, u8> {
    #[inline]
    fn remaining(&self) -> usize {
        SampleBuf::<u8>::remaining(self)
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        SampleBuf::<u8>::chunk(self)
    }

    #[inline]
    fn advance(&mut self, cnt: usize) {
        SampleBuf::<u8>::advance(self, cnt)
    }
}

#[derive(Debug)]
pub struct Sender<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Sender<T>
where
    T: Clone,
{
    pub fn send(&mut self, data: &[T]) -> Result<(), NoReceivers> {
        let mut state_guard = self.shared.state.write();

        if state_guard.receiver_count == 0 {
            // no receivers are here to ever observe the bytes we would
            // write, so we don't.

            // technically, since there are no receivers that keep track of
            // this, we don't need to increase this
            state_guard.head_pos += data.len();

            Err(NoReceivers)
        }
        else {
            if !data.is_empty() {
                // skip anything that would not end up in the buffer anyway
                let skip = data.len().saturating_sub(state_guard.buffer.capacity());

                // truncate to not exceed capacity
                let truncate = (data.len() + state_guard.buffer.len())
                    .saturating_sub(state_guard.buffer.capacity());
                let keep = state_guard.buffer.capacity().saturating_sub(truncate);

                // if we're skipping bytes, we can't be keeping anything in the
                // ring buffer, because the new data already exceeds the
                // capacity (skip != 0) -> (keep == 0) - and x -> y = !x || y
                debug_assert!(skip == 0 || keep == 0);

                // prepare buffer
                state_guard.buffer.retain_back(keep);

                // skip anything in input that would be dropped anyway
                let data = &data[skip..];

                // write data to buffer
                let capacity_before = state_guard.buffer.capacity();
                state_guard.buffer.extend(data.iter().cloned());
                debug_assert_eq!(state_guard.buffer.capacity(), capacity_before);

                // update head position
                state_guard.head_pos += data.len();

                // notify receivers
                state_guard.wake_all_receivers();
            }

            Ok(())
        }
    }

    pub fn poll_wait_receivers(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Result<(), NoSubscribers>> {
        let mut state_guard = self.shared.state.write();

        if state_guard.receiver_count > 0 {
            Poll::Ready(Ok(()))
        }
        else if state_guard.subscriber_count == 0 {
            Poll::Ready(Err(NoSubscribers))
        }
        else {
            let new_waker = cx.waker();
            let slot = state_guard.sender_slot.as_mut().unwrap();
            if let Some(waker) = &mut slot.waker {
                waker.clone_from(new_waker);
            }
            else {
                slot.waker = Some(new_waker.clone());
            }

            Poll::Pending
        }
    }

    pub async fn wait_for_receivers(&mut self) -> Result<(), NoSubscribers> {
        std::future::poll_fn(move |cx| Pin::new(&mut *self).poll_wait_receivers(cx)).await
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut state_guard = self.shared.state.write();
        state_guard.sender_count -= 1;
        state_guard.sender_slot = None;

        state_guard.wake_all_receivers();
    }
}

// todo: T needs to be Unpin so we can deref through Pin. We could use
// pin_project, but would need to put `shared` into a wrapper and put the Drop
// impl on that.
impl<T> AsyncWriteSamples<T> for Sender<T>
where
    T: Unpin + Clone,
{
    type Error = !;

    fn poll_write_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[T],
    ) -> Poll<Result<usize, Self::Error>> {
        // <https://github.com/jgraef/mrrp/issues/30>
        let coop = ready!(coop::poll_proceed(cx));
        coop.made_progress();

        let n = match Pin::get_mut(self).send(buffer) {
            Ok(()) => buffer.len(),
            Err(NoReceivers) => 0,
        };

        Poll::Ready(Ok(n))
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    #[inline]
    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

#[derive(Debug)]
pub struct Subscriber<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Subscriber<T> {
    pub fn subscribe(&self) -> Result<Receiver<T>, NoSenders> {
        let mut state_guard = self.shared.state.write();

        if state_guard.sender_count == 0 {
            Err(NoSenders)
        }
        else {
            Ok(Receiver {
                inner: ReceiverInner {
                    shared: self.shared.clone(),
                    slot: state_guard.subscribe(),
                    read_pos: state_guard.head_pos,
                },
            })
        }
    }
}

impl<T> Clone for Subscriber<T> {
    fn clone(&self) -> Self {
        let mut state_guard = self.shared.state.write();
        state_guard.subscriber_count += 1;

        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<T> Drop for Subscriber<T> {
    fn drop(&mut self) {
        let mut state_guard = self.shared.state.write();
        state_guard.subscriber_count -= 1;
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZero;

    use futures_util::future;

    use crate::signal::{
        AsyncReadSamplesExt,
        EofError,
        broadcast::pipe,
        write::AsyncWriteSamplesExt,
    };

    #[tokio::test]
    async fn send_to_single_receiver() {
        let (mut sender, subscriber) = pipe(NonZero::new(1024).unwrap());
        let mut receiver = subscriber.subscribe().unwrap();

        let data = (0..10).collect::<Vec<_>>();
        sender.write_all(&data).await.unwrap();

        let mut buffer = vec![0; 1024];
        let n = receiver.read_samples(&mut buffer).await.unwrap();
        assert_eq!(n, 10);
        assert_eq!(buffer[..10], data);
    }

    #[tokio::test]
    async fn send_to_multiple_receiver() {
        let (mut sender, subscriber) = pipe(NonZero::new(1024).unwrap());
        let mut receiver = subscriber.subscribe().unwrap();
        let mut receiver2 = subscriber.subscribe().unwrap();

        let data = (0..10).collect::<Vec<_>>();
        sender.write_all(&data).await.unwrap();

        let mut buffer = vec![0; 1024];
        let n = receiver.read_samples(&mut buffer).await.unwrap();
        assert_eq!(n, 10);
        assert_eq!(buffer[..10], data);

        let n = receiver2.read_samples(&mut buffer).await.unwrap();
        assert_eq!(n, 10);
        assert_eq!(buffer[..10], data);
    }

    #[tokio::test]
    async fn send_receive_twice() {
        let (mut sender, subscriber) = pipe(NonZero::new(1024).unwrap());
        let mut receiver = subscriber.subscribe().unwrap();

        let data = (0..10).collect::<Vec<_>>();
        sender.write_all(&data).await.unwrap();

        let mut buffer = vec![0; 10];
        let n = receiver.read_samples(&mut buffer).await.unwrap();
        assert_eq!(n, 10);
        assert_eq!(buffer, data);

        sender.write_all(&data).await.unwrap();

        receiver.read_samples(&mut buffer).await.unwrap();
        assert_eq!(n, 10);
        assert_eq!(buffer, data);
    }

    #[tokio::test]
    async fn send_dropping() {
        let (mut sender, subscriber) = pipe(NonZero::new(10).unwrap());
        let mut receiver = subscriber.subscribe().unwrap();

        let data = (0..20).collect::<Vec<_>>();
        sender.write_all(&data).await.unwrap();

        // since the pipe only has 10 samples of buffering, but 20 have already
        // been sent, we'll only see the last 10.
        let mut buffer = vec![0; 1024];
        let n = receiver.read_samples(&mut buffer).await.unwrap();
        assert_eq!(n, 10);
        assert_eq!(buffer[..10], data[10..]);
    }

    #[tokio::test]
    async fn pipe_closing() {
        let (mut sender, subscriber) = pipe(NonZero::new(1024).unwrap());
        let mut receiver = subscriber.subscribe().unwrap();

        let data = (0..10).collect::<Vec<_>>();
        sender.write_all(&data).await.unwrap();
        drop(sender);

        let mut buffer = vec![0; 1024];
        let n = receiver.read_samples(&mut buffer).await.unwrap();
        assert_eq!(n, 10);
        assert_eq!(buffer[..10], data);

        let err = receiver.read_sample().await.unwrap_err();
        match err {
            // since the other variant is unhibited. this is the only possible
            // case.
            EofError::Eof { num_samples_read } => {
                assert_eq!(num_samples_read, 0);
            }
        }
    }

    #[tokio::test]
    async fn async_broadcast() {
        let (mut sender, subscriber) = pipe(NonZero::new(1024).unwrap());

        let mut join_handles = vec![];

        join_handles.push(tokio::spawn(async move {
            sender.wait_for_receivers().await.unwrap();

            let mut buf = vec![0; 16];
            let mut i = 0;

            while i < 0x10_000 {
                buf.fill_with(|| {
                    let x = i;
                    i += 1;
                    x
                });

                sender.send(&buf).unwrap();
                tokio::task::yield_now().await;
            }
        }));

        for _i in 0..4 {
            let mut receiver = subscriber.subscribe().unwrap();
            join_handles.push(tokio::spawn(async move {
                let mut buf = vec![0; 1024];
                let mut i = 0;
                loop {
                    let n = receiver.read_samples(&mut buf).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    for x in &buf[..n] {
                        assert_eq!(*x, i);
                        i += 1;
                    }
                }
                assert_eq!(i, 0x10_000);
            }));
        }

        future::try_join_all(join_handles).await.unwrap();
    }
}
