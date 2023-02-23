#![feature(generators, generator_trait)]

use std::ops::{Generator, GeneratorState};
use std::pin::Pin;

fn main() {
    let mut generator1 = || {
        yield 1;
        "foo"
    };
    let mut generator2 = || {
        yield 2;
        "ha"
    };

    match Pin::new(&mut generator1).resume(()) {
        GeneratorState::Yielded(1) => {}
        _ => panic!("unexpected return from resume"),
    }
    match Pin::new(&mut generator2).resume(()) {
        GeneratorState::Yielded(2) => {}
        _ => panic!("unexpected return from resume"),
    }
    match Pin::new(&mut generator1).resume(()) {
        GeneratorState::Complete("foo") => {}
        _ => panic!("unexpected return from resume"),
    }
    match Pin::new(&mut generator2).resume(()) {
        GeneratorState::Complete("ha") => {}
        _ => panic!("unexpected return from resume"),
    }
}