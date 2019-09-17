//! Serialize a Rust data structure to Dfinity IDL

use super::error::{Error, Result};

use std::io;
use std::vec::Vec;
use std::collections::HashMap;
use dfx_info::types::{Type, Field};

use leb128::write::{signed as sleb128_encode, unsigned as leb128_encode};

/// Serializes a value to a vector.
pub fn to_vec<T>(value: &T) -> Result<Vec<u8>>
where
    T: dfx_info::IDLType,
{
    let mut vec = Vec::new();
    to_writer(&mut vec, value)?;
    Ok(vec)
}

/// Serializes a value to a writer.
pub fn to_writer<W, T>(mut writer: W, value: &T) -> Result<()>
where
    W: io::Write,
    T: dfx_info::IDLType,
{
    writer.write_all(b"DIDL")?;
    
    let mut type_ser = TypeSerialize::new();
    type_ser.serialize(&T::ty())?;
    writer.write_all(&type_ser.result)?;
    
    let mut value_ser = ValueSerializer::new();
    value.idl_serialize(&mut value_ser)?;
    writer.write_all(&value_ser.value)?;
    Ok(())
}

/// A structure for serializing Rust values to IDL.
#[derive(Debug)]
pub struct ValueSerializer {
    value: Vec<u8>,
}

impl ValueSerializer
{
    /// Creates a new IDL serializer.
    #[inline]
    pub fn new() -> Self {
        ValueSerializer {
            value: Vec::new()
        }
    }

    fn write_sleb128(&mut self, value: i64) -> () {
        sleb128_encode(&mut self.value, value).unwrap();
    }
    fn write_leb128(&mut self, value: u64) -> () {
        leb128_encode(&mut self.value, value).unwrap();
    }
}

impl<'a> dfx_info::Serializer for &'a mut ValueSerializer {
    type Error = Error;
    type Compound = Compound<'a>;
    fn serialize_bool(self, v: bool) -> Result<()> {
        let v = if v { 1 } else { 0 };
        Ok(self.write_leb128(v))
    }
    fn serialize_int(self, v: i64) -> Result<()> {
        Ok(self.write_sleb128(v))
    }
    fn serialize_nat(self, v: u64) -> Result<()> {
        Ok(self.write_leb128(v))
    }
    fn serialize_text(self, v: &str) -> Result<()> {
        let mut buf = Vec::from(v.as_bytes());
        self.write_leb128(buf.len() as u64);
        self.value.append(&mut buf);
        Ok(())        
    }
    fn serialize_null(self, _v:()) -> Result<()> {
        Ok(())
    }
    fn serialize_option<T: ?Sized>(self, v: Option<&T>) -> Result<()>
    where T: dfx_info::IDLType {
        match v {
            None => Ok(self.write_leb128(0)),
            Some(v) => {
                self.write_leb128(1);
                v.idl_serialize(self)
            }
        }
    }
    fn serialize_variant(self, index: u64) -> Result<Self::Compound> {
        self.write_leb128(index);
        Ok(Self::Compound { ser: self })
    }    
    fn serialize_struct(self) -> Result<Self::Compound> {
        Ok(Self::Compound { ser: self })
    }
    fn serialize_vec(self, len: usize) -> Result<Self::Compound> {
        self.write_leb128(len as u64);
        Ok(Self::Compound { ser: self })
    }
}

pub struct Compound<'a> { ser: &'a mut ValueSerializer }
impl<'a> dfx_info::Compound for Compound<'a> {
    type Error = Error;
    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<()>
    where
        T: dfx_info::IDLType,
    {
        value.idl_serialize(&mut *self.ser)?;
        Ok(())
    }    
}

/// A structure for serializing Rust values to IDL types.
#[derive(Debug)]
pub struct TypeSerialize {
    type_table: Vec<Vec<u8>>,
    type_map: HashMap<Type, i32>,
    result: Vec<u8>,
}

impl TypeSerialize
{
    #[inline]
    pub fn new() -> Self {
        TypeSerialize {
            type_table: Vec::new(),
            type_map: HashMap::new(),
            result: Vec::new()
        }
    }

    #[inline]
    fn build_type(&mut self, t: &Type) -> Result<()> {
        if !dfx_info::types::is_primitive(t) && !self.type_map.contains_key(t) {
            // This is a hack to remove (some) equivalent mu types
            // from the type table.
            // Someone should implement Pottier's O(nlogn) algorithm
            // http://gallium.inria.fr/~fpottier/publis/gauthier-fpottier-icfp04.pdf
            let unrolled = dfx_info::types::unroll(t);
            if let Some(idx) = self.type_map.get(&unrolled) {
                let idx = idx.clone();
                self.type_map.insert((*t).clone(), idx);
                return Ok(());
            }
            
            let idx = self.type_table.len();
            self.type_map.insert((*t).clone(), idx as i32);
            self.type_table.push(Vec::new());
            let mut buf = Vec::new();
            match t {
                Type::Opt(ref ty) => {
                    self.build_type(ty)?;
                    sleb128_encode(&mut buf, -18)?;
                    self.encode(&mut buf, ty)?;
                },
                Type::Vec(ref ty) => {
                    self.build_type(ty)?;
                    sleb128_encode(&mut buf, -19)?;
                    self.encode(&mut buf, ty)?;
                },                
                Type::Record(fs) => {
                    for Field {id:_,hash:_,ty} in fs.iter() {
                        self.build_type(ty).unwrap();
                    };
                    
                    sleb128_encode(&mut buf, -20)?;
                    leb128_encode(&mut buf, fs.len() as u64)?;
                    for Field {id:_,hash,ty} in fs.iter() {
                        leb128_encode(&mut buf, *hash as u64)?;
                        self.encode(&mut buf, ty)?;
                    };
                },
                Type::Variant(fs) => {
                    for Field{id:_,hash:_,ty} in fs.iter() {
                        self.build_type(ty).unwrap();
                    };
                    
                    sleb128_encode(&mut buf, -21)?;
                    leb128_encode(&mut buf, fs.len() as u64)?;
                    for Field{id:_,hash,ty} in fs.iter() {
                        leb128_encode(&mut buf, *hash as u64)?;
                        self.encode(&mut buf, ty)?;
                    };
                },                
                _ => panic!("unreachable"),
            };
            self.type_table[idx] = buf;
        };
        Ok(())
    }

    fn encode(&mut self, buf: &mut Vec<u8>, t: &Type) -> Result<()> {
        match t {
            Type::Null => sleb128_encode(buf, -1),
            Type::Bool => sleb128_encode(buf, -2),
            Type::Nat => sleb128_encode(buf, -3),
            Type::Int => sleb128_encode(buf, -4),
            Type::Text => sleb128_encode(buf, -15),
            Type::Knot(id) => {
                let ty = dfx_info::types::find_type(id)
                    .expect("knot TypeId not found");
                let idx = self.type_map.get(&ty)
                    .expect(&format!("knot type {:?} not found", ty));
                sleb128_encode(buf, *idx as i64)
            },
            _ => {
                let idx = self.type_map.get(&t)
                    .expect(&format!("type {:?} not found", t));
                sleb128_encode(buf, *idx as i64)
            },
        }?;
        Ok(())
    }

    fn serialize(&mut self, t: &Type) -> Result<()> {
        self.build_type(t)?;
        //println!("{:?}", self.type_map);

        leb128_encode(&mut self.result, self.type_table.len() as u64)?;
        self.result.append(&mut self.type_table.concat());
        let mut ty_encode = Vec::new();        
        self.encode(&mut ty_encode, t)?;
        self.result.append(&mut ty_encode);
        Ok(())
    }
}

