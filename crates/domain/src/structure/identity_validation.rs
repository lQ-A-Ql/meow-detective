use serde::ser::{
    SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};
use serde::{Serialize, Serializer};

use super::canonical_identity::CanonicalIdentityError;

#[derive(Default)]
pub(super) struct IdentityValidation {
    depth: usize,
    values: usize,
}

impl IdentityValidation {
    fn enter(&mut self) -> Result<(), CanonicalIdentityError> {
        if self.depth >= 64 {
            return Err(CanonicalIdentityError::StructuralLimit);
        }
        self.depth += 1;
        Ok(())
    }

    fn value(&mut self) -> Result<(), CanonicalIdentityError> {
        self.values += 1;
        if self.values > 65_536 {
            return Err(CanonicalIdentityError::StructuralLimit);
        }
        Ok(())
    }
}

macro_rules! scalar {
    ($($method:ident($type:ty)),+ $(,)?) => {
        $(fn $method(self, _: $type) -> Result<(), Self::Error> {
            self.value()
        })+
    };
}

impl Serializer for &mut IdentityValidation {
    type Ok = ();
    type Error = CanonicalIdentityError;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    scalar! {
        serialize_bool(bool), serialize_i8(i8), serialize_i16(i16), serialize_i32(i32),
        serialize_i64(i64), serialize_u8(u8), serialize_u16(u16), serialize_u32(u32),
        serialize_u64(u64), serialize_char(char), serialize_str(&str), serialize_bytes(&[u8]),
    }

    fn serialize_f32(self, _: f32) -> Result<(), Self::Error> {
        Err(CanonicalIdentityError::FloatingPoint)
    }

    fn serialize_f64(self, _: f64) -> Result<(), Self::Error> {
        Err(CanonicalIdentityError::FloatingPoint)
    }

    fn serialize_none(self) -> Result<(), Self::Error> {
        self.value()
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), Self::Error> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<(), Self::Error> {
        self.value()
    }

    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Self::Error> {
        self.value()
    }

    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
    ) -> Result<(), Self::Error> {
        self.value()
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        value.serialize(self)
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Self, Self::Error> {
        self.value()?;
        self.enter()?;
        Ok(self)
    }

    fn serialize_tuple(self, length: usize) -> Result<Self, Self::Error> {
        self.serialize_seq(Some(length))
    }

    fn serialize_tuple_struct(self, _: &'static str, length: usize) -> Result<Self, Self::Error> {
        self.serialize_seq(Some(length))
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        length: usize,
    ) -> Result<Self, Self::Error> {
        self.serialize_seq(Some(length))
    }

    fn serialize_map(self, length: Option<usize>) -> Result<Self, Self::Error> {
        self.serialize_seq(length)
    }

    fn serialize_struct(self, _: &'static str, length: usize) -> Result<Self, Self::Error> {
        self.serialize_seq(Some(length))
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        length: usize,
    ) -> Result<Self, Self::Error> {
        self.serialize_seq(Some(length))
    }
}

macro_rules! sequence {
    ($trait:ident, $method:ident) => {
        impl $trait for &mut IdentityValidation {
            type Ok = ();
            type Error = CanonicalIdentityError;

            fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
                value.serialize(&mut **self)
            }

            fn end(self) -> Result<(), Self::Error> {
                self.depth -= 1;
                Ok(())
            }
        }
    };
}

sequence!(SerializeSeq, serialize_element);
sequence!(SerializeTuple, serialize_element);
sequence!(SerializeTupleStruct, serialize_field);
sequence!(SerializeTupleVariant, serialize_field);

impl SerializeMap for &mut IdentityValidation {
    type Ok = ();
    type Error = CanonicalIdentityError;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Self::Error> {
        key.serialize(&mut **self)
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<(), Self::Error> {
        self.depth -= 1;
        Ok(())
    }
}

macro_rules! structure {
    ($trait:ident) => {
        impl $trait for &mut IdentityValidation {
            type Ok = ();
            type Error = CanonicalIdentityError;

            fn serialize_field<T: Serialize + ?Sized>(
                &mut self,
                _: &'static str,
                value: &T,
            ) -> Result<(), Self::Error> {
                value.serialize(&mut **self)
            }

            fn end(self) -> Result<(), Self::Error> {
                self.depth -= 1;
                Ok(())
            }
        }
    };
}

structure!(SerializeStruct);
structure!(SerializeStructVariant);
