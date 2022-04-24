use std::{cell::RefCell, rc::Rc};

use crate::{server_addr::ServerAddr, wasm_utils::candidate_to_addr};

// MaybeAddr
struct MaybeAddr(pub ServerAddr);

// AddrCell
#[derive(Clone)]
pub struct AddrCell {
    cell: Rc<RefCell<MaybeAddr>>,
}

impl Default for AddrCell {
    fn default() -> Self {
        AddrCell {
            cell: Rc::new(RefCell::new(MaybeAddr(ServerAddr::Finding))),
        }
    }
}

impl AddrCell {
    pub fn receive_candidate(&self, candidate_str: &str) {
        self.cell
            .as_ref()
            .try_borrow_mut()
            .expect("cannot borrow AddrCell.cell!")
            .0 = candidate_to_addr(candidate_str);
    }

    pub fn get(&self) -> ServerAddr {
        self.cell.as_ref().borrow().0
    }
}
