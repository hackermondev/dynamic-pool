use std::time::Duration;

use dynamic_pool::{DynamicPool, DynamicPoolConfig, DynamicReset};

#[derive(Default, Debug)]
struct Person {
    name: String,
    age: u16,
}

impl DynamicReset for Person {
    fn reset(&mut self) {
        self.name.clear();
        self.age = 0;
    }
}

#[test]
fn create_and_insert() {
    let pool = DynamicPool::new(DynamicPoolConfig {
        max_capacity: 10,
        ..Default::default()
    });

    assert_eq!(pool.used(&"hello_world"), 0);
    assert!(pool.try_take(&"hello_world").is_none());

    pool.insert("hello_world", Person::default()).unwrap();

    let _person = pool.try_take(&"hello_world").unwrap();
    assert_eq!(pool.used(&"hello_world"), 1);
    assert!(pool.try_take(&"hello_world").is_none());
}

#[test]
fn reset() {
    let pool = DynamicPool::new(DynamicPoolConfig {
        max_capacity: 10,
        ..Default::default()
    });

    pool.insert("hello_world", Person::default()).unwrap();

    let mut person = pool.try_take(&"hello_world").unwrap();
    person.age = 100;
    person.name = String::from("Elon Musk");
    drop(person);

    let person = pool.try_take(&"hello_world").unwrap();
    assert!(person.name.is_empty());
    assert_eq!(person.age, 0);
}

#[tokio::test]
async fn time_to_live() {
    let pool = DynamicPool::new(DynamicPoolConfig {
        max_capacity: 10,
        time_to_live: Some(Duration::from_millis(10)),
    });

    pool.insert("hello_world", Person::default()).unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(pool.used(&"hello_world"), 0);
    assert!(pool.try_take(&"hello_world").is_none());
}
