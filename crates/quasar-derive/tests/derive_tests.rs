//! Derive macro unit tests

use quasar_derive::Inspect;

#[test]
fn inspect_derive_struct() {
    #[derive(Inspect, Debug, Clone)]
    struct TestStruct {
        value: f32,
        name: String,
    }

    let _item = TestStruct {
        value: 42.0,
        name: "test".to_string(),
    };

    // Just verify it compiles and derives Inspect
}

#[test]
fn inspect_derive_unit_struct() {
    #[derive(Inspect, Debug, Clone)]
    struct UnitStruct;

    let _item = UnitStruct;
}

#[test]
fn inspect_derive_tuple_struct() {
    #[derive(Inspect, Debug, Clone)]
    struct TupleStruct(f32, f32, f32);

    let _item = TupleStruct(1.0, 2.0, 3.0);
}

#[test]
fn inspect_derive_nested() {
    #[derive(Inspect, Debug, Clone)]
    struct Inner {
        x: f32,
    }

    #[derive(Inspect, Debug, Clone)]
    struct Outer {
        inner: Inner,
    }

    let _item = Outer {
        inner: Inner { x: 1.0 },
    };
}

#[test]
fn inspect_derive_with_option() {
    #[derive(Inspect, Debug, Clone)]
    struct OptionalField {
        value: Option<f32>,
    }

    let _item = OptionalField { value: Some(1.0) };
    let _item2 = OptionalField { value: None };
}

#[test]
fn inspect_derive_with_vec() {
    #[derive(Inspect, Debug, Clone)]
    struct VecField {
        items: Vec<f32>,
    }

    let _item = VecField {
        items: vec![1.0, 2.0, 3.0],
    };
}
