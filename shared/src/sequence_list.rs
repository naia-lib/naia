use crate::sequence_less_than;

pub struct SequenceList<T> {
    list: Vec<(u16, T)>,
}

impl<T> SequenceList<T> {
    pub fn new() -> Self {
        Self { list: Vec::new() }
    }

    pub fn front(&self) -> Option<&(u16, T)> {
        self.list.first()
    }

    pub fn pop_front(&mut self) -> (u16, T) {
        self.list.remove(0)
    }

    pub fn contains_scan_from_back(&self, id: u16) -> bool {
        for (old_id, _) in self.list.iter().rev() {
            if *old_id == id {
                return true;
            }
            if sequence_less_than(*old_id, id) {
                return false;
            }
        }
        // made it all the way through
        false
    }

    pub fn get_scan_from_back(&self, id: u16) -> Option<&T> {
        for (old_id, item) in self.list.iter().rev() {
            if *old_id == id {
                return Some(item);
            }
            if sequence_less_than(*old_id, id) {
                return None;
            }
        }
        // made it all the way through
        None
    }

    pub fn get_mut_scan_from_back(&mut self, id: u16) -> Option<&mut T> {
        for (old_id, item) in self.list.iter_mut().rev() {
            if *old_id == id {
                return Some(item);
            }
            if sequence_less_than(*old_id, id) {
                return None;
            }
        }
        // made it all the way through
        None
    }

    pub fn insert_scan_from_back(&mut self, id: u16, item: T) {
        for (index, (old_id, _)) in self.list.iter().enumerate().rev() {
            assert!(*old_id != id, "duplicates are not allowed");
            if sequence_less_than(*old_id, id) {
                self.list.insert(index + 1, (id, item));
                return;
            }
        }
        // made it all the way through, insert at front and be done
        self.list.insert(0, (id, item));
    }

    pub fn remove_scan_from_front(&mut self, id: u16) -> Option<T> {
        let index = self.list.iter().position(|(old_id, _)| *old_id == id)?;
        Some(self.list.remove(index).1)
    }

    pub fn iter(&self) -> impl Iterator<Item = &(u16, T)> {
        self.list.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut (u16, T)> {
        self.list.iter_mut()
    }
}
