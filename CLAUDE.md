# Best pracrtices

## Basic

- Always search the web to get latest deps and its usage.
- follow SOLID and DRY principles.
- Do not write any TODO or unfinished code, do not write temporary solution. If you has such situation, stop current task and review the problem globally,start thinking about the design and the best alternative solutions.
- DO NOT change model, always use gemini-3-pro-preview. Do not try to avoid tool use. Always seek concrete solutions. Search the web for concrete solutions.for (let i = 0; i < 5; i++) {
  console.log('Hello world!');
}

## Rust

- Always do `cargo build`, `cargo test`, `cargo fmt`, and `cargo clippy` before finishing the task.
- Use ergonomic rust, always follow best practices.
- Use Dashmap rather than Mutext/RwLock of HashMap.
- use ArcSwap for data seldom change, e.g. config.
- use channels over shared memory.
- always use rustls and awc-lc-rs for anything that related to TLS.
- Rust has already supported async trait, so no need to use async-trait crate any more.
- always use serde rename / alias for field name mapping.
- bring in typed-builder crate for builder pattern. Use it for complex structs.
