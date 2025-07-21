use std::{
    fmt::Debug,
    mem::MaybeUninit,
};

pub struct ArrayVecDeque<const N: usize, T> {
    items: [MaybeUninit<T>; N],
    head: usize,
    length: usize,
}

impl<const N: usize, T> ArrayVecDeque<N, T> {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.length
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.length == N
    }

    #[inline]
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    pub fn as_slices(&self) -> (&[T], &[T]) {
        let (head_length, tail_length) = self.slice_lengths();
        let (tail, head) = self.items.split_at(self.head);

        let head = unsafe { head[..head_length].assume_init_ref() };
        let tail = unsafe { tail[..tail_length].assume_init_ref() };

        (head, tail)
    }

    pub fn as_mut_slices(&mut self) -> (&mut [T], &mut [T]) {
        let (head_length, tail_length) = self.slice_lengths();
        let (tail, head) = self.items.split_at_mut(self.head);

        let head = unsafe { head[..head_length].assume_init_mut() };
        let tail = unsafe { tail[..tail_length].assume_init_mut() };

        (head, tail)
    }

    #[inline]
    pub fn front(&self) -> Option<&T> {
        Some(unsafe { self.items[front_index(self.head, self.length, N)?].assume_init_ref() })
    }

    #[inline]
    pub fn front_mut(&mut self) -> Option<&mut T> {
        Some(unsafe { self.items[front_index(self.head, self.length, N)?].assume_init_mut() })
    }

    #[inline]
    pub fn pop_front(&mut self) -> Option<T> {
        let index = front_index(self.head, self.length, N)?;
        self.increment_head();
        self.length -= 1;
        Some(unsafe { self.items[index].assume_init_read() })
    }

    pub fn push_front(&mut self, value: T) -> Option<T> {
        todo!("fixme");
        self.decrement_head();

        let old_value = if self.length == N {
            Some(unsafe { self.items[self.head].assume_init_read() })
        }
        else {
            self.length += 1;
            None
        };

        self.items[self.head].write(value);

        old_value
    }

    #[inline]
    pub fn back(&self) -> Option<&T> {
        unsafe { Some(self.items[back_index(self.head, self.length, N)?].assume_init_ref()) }
    }

    #[inline]
    pub fn back_mut(&mut self) -> Option<&mut T> {
        unsafe { Some(self.items[back_index(self.head, self.length, N)?].assume_init_mut()) }
    }

    #[inline]
    pub fn pop_back(&mut self) -> Option<T> {
        let index = back_index(self.head, self.length, N)?;
        self.length -= 1;
        Some(unsafe { self.items[index].assume_init_read() })
    }

    pub fn push_back(&mut self, value: T) -> Option<T> {
        todo!("fixme");
        if self.length == N {
            let old_value = unsafe { self.items[self.head].assume_init_read() };
            self.items[self.head].write(value);
            self.increment_head();
            Some(old_value)
        }
        else {
            self.items[back_index(self.head, self.length, N).unwrap_or_default()].write(value);
            self.length += 1;
            None
        }
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        Some(unsafe { self.items[self.get_index(index)?].assume_init_ref() })
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        Some(unsafe { self.items[self.get_index(index)?].assume_init_mut() })
    }

    pub fn contains(&self, x: &T) -> bool
    where
        T: PartialEq,
    {
        self.iter().any(|y| y == x)
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            items: &self.items,
            index: self.head,
            length: self.length,
        }
    }

    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            items: &mut self.items,
            index: self.head,
            length: self.length,
        }
    }

    #[inline]
    fn slice_lengths(&self) -> (usize, usize) {
        let head_length = (N - self.head).min(self.length);
        let tail_length = self.length - head_length;
        (head_length, tail_length)
    }

    #[inline]
    fn decrement_head(&mut self) {
        self.head = self.head.checked_sub(1).unwrap_or(N - 1);
    }

    #[inline]
    fn increment_head(&mut self) {
        self.head += 1;
        if self.head == N {
            self.head = 0;
        }
    }

    #[inline]
    fn get_index(&self, mut index: usize) -> Option<usize> {
        (index < self.length).then(|| {
            index += self.head;
            if index >= N {
                index -= N;
            }
            index
        })
    }
}

impl<const N: usize, T> Drop for ArrayVecDeque<N, T> {
    fn drop(&mut self) {
        while let Some(item) = self.pop_front() {
            drop(item);
        }
    }
}

impl<const N: usize, T> Default for ArrayVecDeque<N, T> {
    #[inline]
    fn default() -> Self {
        Self {
            items: [const { MaybeUninit::uninit() }; N],
            head: 0,
            length: 0,
        }
    }
}

impl<const N: usize, T> Clone for ArrayVecDeque<N, T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        let mut items = [const { MaybeUninit::uninit() }; N];
        let (first, second) = self.as_slices();
        items[..first.len()].write_clone_of_slice(first);
        items[first.len()..][..second.len()].write_clone_of_slice(second);

        Self {
            items,
            head: 0,
            length: self.length,
        }
    }
}

impl<const N: usize, T> Debug for ArrayVecDeque<N, T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self).finish()
    }
}

#[derive(Clone, Copy)]
pub struct Iter<'a, T> {
    items: &'a [MaybeUninit<T>],
    index: usize,
    length: usize,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let index = front_index(self.index, self.length, self.items.len())?;
        self.index += 1;
        if self.index == self.items.len() {
            self.index = 0;
        }
        self.length -= 1;
        Some(unsafe { self.items[index].assume_init_ref() })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.length, Some(self.length))
    }
}

impl<'a, T> DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let index = back_index(self.index, self.length, self.items.len())?;
        self.length -= 1;
        Some(unsafe { self.items[index].assume_init_ref() })
    }
}

impl<'a, T> ExactSizeIterator for Iter<'a, T> {}

impl<'a, const N: usize, T> IntoIterator for &'a ArrayVecDeque<N, T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct IterMut<'a, T> {
    items: &'a mut [MaybeUninit<T>],
    index: usize,
    length: usize,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        let index = front_index(self.index, self.length, self.items.len())?;
        self.index += 1;
        if self.index == self.items.len() {
            self.index = 0;
        }
        self.length -= 1;
        Some(unsafe { &mut *(self.items[index].as_mut_ptr()) })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.length, Some(self.length))
    }
}

impl<'a, T> ExactSizeIterator for IterMut<'a, T> {}

impl<'a, T> DoubleEndedIterator for IterMut<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let index = back_index(self.index, self.length, self.items.len())?;
        self.length -= 1;
        Some(unsafe { &mut *(self.items[index].as_mut_ptr()) })
    }
}

impl<'a, const N: usize, T> IntoIterator for &'a mut ArrayVecDeque<N, T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[derive(Clone, Debug)]
pub struct IntoIter<const N: usize, T> {
    inner: ArrayVecDeque<N, T>,
}

impl<const N: usize, T> Iterator for IntoIter<N, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.pop_front()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.inner.length, Some(self.inner.length))
    }
}

impl<const N: usize, T> DoubleEndedIterator for IntoIter<N, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.pop_back()
    }
}

impl<const N: usize, T> ExactSizeIterator for IntoIter<N, T> {}

impl<const N: usize, T> IntoIterator for ArrayVecDeque<N, T> {
    type Item = T;
    type IntoIter = IntoIter<N, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        IntoIter { inner: self }
    }
}

#[inline]
fn front_index(head: usize, length: usize, _capacity: usize) -> Option<usize> {
    (length != 0).then_some(head)
}

#[inline]
fn back_index(head: usize, length: usize, capacity: usize) -> Option<usize> {
    dbg!(head, length, capacity);
    let mut index = head + length.checked_sub(1)?;
    if index >= capacity {
        index -= capacity;
    }
    Some(index)
}

#[cfg(test)]
mod tests {
    use crate::util::array_vecdeque::ArrayVecDeque;

    #[test]
    fn basic_operations() {
        let mut deque = ArrayVecDeque::<4, i32>::new();

        assert_eq!(deque.push_back(1), None);
        assert_eq!(deque.head, 0);
        assert_eq!(deque.length, 1);
        assert_eq!(deque.front(), Some(&1));
        assert_eq!(deque.back(), Some(&1));
        assert_eq!(deque.len(), 1);

        assert_eq!(deque.push_front(2), None);
        assert_eq!(deque.head, 3);
        assert_eq!(deque.length, 2);
        assert_eq!(deque.front(), Some(&2));
        assert_eq!(deque.back(), Some(&1));
        assert_eq!(deque.len(), 2);

        assert_eq!(deque.push_back(3), None);
        assert_eq!(deque.head, 3);
        assert_eq!(deque.length, 3);
        assert_eq!(deque.front(), Some(&2));
        assert_eq!(deque.back(), Some(&3));
        /*assert_eq!(deque.len(), 3);

        assert_eq!(deque.iter().copied().collect::<Vec<_>>(), vec![2, 1, 3]);
        assert_eq!(deque.get(0), Some(&2));
        assert_eq!(deque.get(1), Some(&1));
        assert_eq!(deque.get(2), Some(&3));
        assert_eq!(deque.get(3), None);

        assert_eq!(deque.push_back(4), None);
        assert_eq!(deque.front(), Some(&2));
        assert_eq!(deque.back(), Some(&4));
        assert_eq!(deque.len(), 4);

        assert_eq!(deque.push_back(5), Some(2));
        assert_eq!(deque.front(), Some(&2));
        assert_eq!(deque.back(), Some(&4));
        assert_eq!(deque.len(), 4);

        assert_eq!(deque.pop_front(), Some(1));
        assert_eq!(deque.pop_back(), Some(5));*/
    }
}
