//! Shared support for the adapter integration tests. Not a test target: it lives in a subdirectory
//! so Cargo only compiles it into the tests that declare `mod adapter_support;`.

#![allow(dead_code)]

pub mod claude_fixture;
pub mod conformance;
pub mod pi_fixture;
pub mod runtime;
