
use std::io::Read;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use crate::{NetType, NetTypeTrait, NetBase, NetEvent, ManagerType};

pub struct Manifest {
    gaia_id_count: u16,
    gaia_id_map: HashMap<u16, Box<dyn NetTypeTrait>>,
    type_id_map: HashMap<TypeId, u16>,
}

impl Manifest {
    pub fn new() -> Self {
        Manifest {
            gaia_id_count: 111,
            gaia_id_map: HashMap::new(),
            type_id_map: HashMap::new()
        }
    }

    pub fn register_type<T: NetBase>(&mut self, some_type: T) {
        let mut net_type_boxed = NetType::init(some_type);
        let new_gaia_id = self.gaia_id_count;
        net_type_boxed.as_mut().set_gaia_id(new_gaia_id);
        self.gaia_id_map.insert(new_gaia_id, net_type_boxed);
        self.gaia_id_count += 1;

        self.type_id_map.insert(TypeId::of::<T>(), new_gaia_id);
    }

    pub fn write_gaia_id<T: NetBase>(&mut self, net_base: T, out_bytes: &mut Vec<u8>) {
        let gaia_id = self.type_id_map.get(&TypeId::of::<T>())
            .expect("hey I should get a TypeId here...");
        out_bytes.write_u16::<BigEndian>(*gaia_id).unwrap();
    }

    pub fn read_gaia_id(mut msg: &[u8]) -> u16 {
        let id = msg.read_u16::<BigEndian>().unwrap().into();
        id
    }

    pub fn write_manager_type(manager_type: ManagerType, out_bytes: &mut Vec<u8>) {
        out_bytes.write_u8(manager_type as u8).unwrap();
    }

    pub fn read_manager_type(mut msg: &[u8]) -> ManagerType {
        let m_type: ManagerType = msg.read_u8().unwrap().into();
        return m_type;
    }

    pub fn write_u8(n: u8, out_bytes: &mut Vec<u8>) {
        out_bytes.write_u8(n);
    }

    pub fn read_u8(mut msg: &[u8]) -> u8 {
        let val: u8 = msg.read_u8().unwrap();
        val
    }

    pub fn write_u16(n: u16, out_bytes: &mut Vec<u8>) {
        out_bytes.write_u16::<BigEndian>(n);
    }

    pub fn read_u16(mut msg: &[u8]) -> u16 {
        let val: u16 = msg.read_u16::<BigEndian>().unwrap();
        val
    }

    pub fn write_test(out_bytes: &mut Vec<u8>) {
        out_bytes.write_u8(13);
        out_bytes.write_u16::<BigEndian>(4815);
        out_bytes.write_u32::<BigEndian>(48151623);
    }

    pub fn read_test(mut msg: &[u8]) {
        let thirteen = msg.read_u8().unwrap();
        let three_numbers = msg.read_u16::<BigEndian>().unwrap();
        let all_number = msg.read_u32::<BigEndian>().unwrap();
        let some_number = all_number;
    }

    pub fn process(&mut self) {

    }
}