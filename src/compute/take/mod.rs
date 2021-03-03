// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

//! Defines take kernel for [Array]

use crate::{
    array::Utf8Array,
    error::{ArrowError, Result},
};

use crate::{
    array::{Array, BinaryArray, DictionaryArray, Offset, BooleanArray, PrimitiveArray},
    datatypes::{DataType, IntervalUnit},
};

mod binary;
mod boolean;
mod dict;
mod generic_binary;
mod primitive;
mod utf8;

macro_rules! downcast_take {
    ($type: ty, $values: expr, $indices: expr) => {{
        let values = $values
            .as_any()
            .downcast_ref::<PrimitiveArray<$type>>()
            .expect("Unable to downcast to a primitive array");
        Ok(Box::new(primitive::take::<$type, _>(&values, $indices)?))
    }};
}

macro_rules! downcast_dict_take {
    ($type: ty, $values: expr, $indices: expr) => {{
        let values = $values
            .as_any()
            .downcast_ref::<DictionaryArray<$type>>()
            .expect("Unable to downcast to a primitive array");
        Ok(Box::new(dict::take::<$type, _>(&values, $indices)?))
    }};
}

pub fn take<O: Offset>(values: &dyn Array, indices: &PrimitiveArray<O>) -> Result<Box<dyn Array>> {
    match values.data_type() {
        DataType::Boolean => {
            let values = values.as_any().downcast_ref::<BooleanArray>().unwrap();
            Ok(Box::new(boolean::take::<O>(values, indices)?))
        }
        DataType::Int8 => downcast_take!(i8, values, indices),
        DataType::Int16 => downcast_take!(i16, values, indices),
        DataType::Int32
        | DataType::Date32
        | DataType::Time32(_)
        | DataType::Interval(IntervalUnit::YearMonth) => downcast_take!(i32, values, indices),
        DataType::Int64
        | DataType::Date64
        | DataType::Time64(_)
        | DataType::Duration(_)
        | DataType::Timestamp(_, _) => downcast_take!(i64, values, indices),
        DataType::UInt8 => downcast_take!(u8, values, indices),
        DataType::UInt16 => downcast_take!(u16, values, indices),
        DataType::UInt32 => downcast_take!(u32, values, indices),
        DataType::UInt64 => downcast_take!(u64, values, indices),
        DataType::Float16 => unreachable!(),
        DataType::Float32 => downcast_take!(f32, values, indices),
        DataType::Float64 => downcast_take!(f64, values, indices),
        DataType::Decimal(_, _) => downcast_take!(i128, values, indices),
        DataType::Utf8 => {
            let values = values.as_any().downcast_ref::<Utf8Array<i32>>().unwrap();
            Ok(Box::new(utf8::take::<i32, _>(values, indices)?))
        }
        DataType::LargeUtf8 => {
            let values = values.as_any().downcast_ref::<Utf8Array<i64>>().unwrap();
            Ok(Box::new(utf8::take::<i64, _>(values, indices)?))
        }
        DataType::Binary => {
            let values = values.as_any().downcast_ref::<BinaryArray<i32>>().unwrap();
            Ok(Box::new(binary::take::<i32, _>(values, indices)?))
        }
        DataType::LargeBinary => {
            let values = values.as_any().downcast_ref::<BinaryArray<i64>>().unwrap();
            Ok(Box::new(binary::take::<i64, _>(values, indices)?))
        }
        DataType::Dictionary(key_type, _) => match key_type.as_ref() {
            DataType::Int8 => downcast_dict_take!(i8, values, indices),
            DataType::Int16 => downcast_dict_take!(i16, values, indices),
            DataType::Int32 => downcast_dict_take!(i32, values, indices),
            DataType::Int64 => downcast_dict_take!(i64, values, indices),
            DataType::UInt8 => downcast_dict_take!(u8, values, indices),
            DataType::UInt16 => downcast_dict_take!(u16, values, indices),
            DataType::UInt32 => downcast_dict_take!(u32, values, indices),
            DataType::UInt64 => downcast_dict_take!(u64, values, indices),
            _ => unreachable!(),
        },
        t => unimplemented!("Take not supported for data type {:?}", t),
    }
}

#[inline(always)]
fn maybe_usize<I: Offset>(index: I) -> Result<usize> {
    index
        .to_usize()
        .ok_or_else(|| ArrowError::DictionaryKeyOverflowError)
}

#[cfg(test)]
mod tests {
    use crate::{array::Primitive, buffer::NativeType};

    use super::*;

    fn test_take_primitive<T>(
        data: &[Option<T>],
        indices: &PrimitiveArray<i32>,
        expected_data: &[Option<T>],
        data_type: DataType,
    ) -> Result<()>
    where
        T: NativeType,
    {
        let output = Primitive::<T>::from(data).to(data_type.clone());
        let expected = Primitive::<T>::from(expected_data).to(data_type);
        let output = take(&output, indices)?;
        assert_eq!(expected, output.as_ref());
        Ok(())
    }

    #[test]
    fn test_take_primitive_non_null_indices() {
        let indices = Primitive::<i32>::from_slice(&[0, 5, 3, 1, 4, 2]).to(DataType::Int32);
        test_take_primitive::<i8>(
            &[None, Some(3), Some(5), Some(2), Some(3), None],
            &indices,
            &[None, None, Some(2), Some(3), Some(3), Some(5)],
            DataType::Int8,
        )
        .unwrap();
    }

    #[test]
    fn test_take_primitive_non_null_values() {
        let indices =
            Primitive::<i32>::from(&[Some(3), None, Some(1), Some(3), Some(2)]).to(DataType::Int32);
        test_take_primitive::<i8>(
            &[Some(0), Some(1), Some(2), Some(3), Some(4)],
            &indices,
            &[Some(3), None, Some(1), Some(3), Some(2)],
            DataType::Int8,
        )
        .unwrap();
    }

    fn test_take_utf8<O>(
        data: &[Option<&str>],
        indices: &PrimitiveArray<i32>,
        expected_data: &[Option<&str>],
    ) -> Result<()>
    where
        O: Offset,
    {
        let output = Utf8Array::<O>::from(&data.to_vec());
        let expected = Utf8Array::<O>::from(&expected_data.to_vec());
        let output = take(&output, indices)?;
        assert_eq!(expected, output.as_ref());
        Ok(())
    }

    #[test]
    fn test_utf8_nulls_nulls() {
        let index =
            Primitive::<i32>::from(&[Some(3), None, Some(1), Some(3), Some(4)]).to(DataType::Int32);
        test_take_utf8::<i32>(
            &[Some("one"), None, Some("three"), Some("four"), Some("five")],
            &index,
            &[Some("four"), None, None, Some("four"), Some("five")],
        )
        .unwrap();
    }

    fn test_take_boolean_arrays(
        data: &[Option<bool>],
        indices: &PrimitiveArray<i32>,
        expected_data: &[Option<bool>],
    ) {
        let input = BooleanArray::from(data);
        let expected = BooleanArray::from(expected_data);
        let output = take(&input, indices).unwrap();
        assert_eq!(expected, output.as_ref());
    }

    #[test]
    fn test_take_primitive_bool() {
        let index =
            Primitive::<i32>::from(&[Some(3), None, Some(1), Some(3), Some(2)]).to(DataType::Int32);

        test_take_boolean_arrays(
            &[Some(false), None, Some(true), Some(false), None],
            &index,
            &[Some(false), None, None, Some(false), Some(true)],
        );
        // todo: test branches to cover 100%
        // * (no validity on indexes, no validity on values)
        // * (no validity on indexes, validity on values)
        // * (validity on indexes, validity on values)
        // * (validity on indexes, no validity on values)
    }
}