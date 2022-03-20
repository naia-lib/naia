use std::{cell::RefCell, rc::Rc};

use crate::{server_addr::ServerAddr, wasm_utils::candidate_to_addr};

// MaybeAddr
struct MaybeAddr(pub ServerAddr);

// AddrCell
#[derive(Clone)]
pub struct AddrCell {
    cell: Rc<RefCell<MaybeAddr>>,
}

impl AddrCell {
    pub fn new() -> Self {
        AddrCell {
            cell: Rc::new(RefCell::new(MaybeAddr(ServerAddr::Finding))),
        }
    }

    pub fn receive_candidate(&self, candidate_str: &str) {
        self.cell.as_ref().borrow_mut().0 = candidate_to_addr(candidate_str);
    }

    pub fn get(&self) -> ServerAddr {
        self.cell.as_ref().borrow().0
    }
}
