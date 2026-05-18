//! Shared SDK infrastructure for CLI ↔ skill pipelines.
//!
//! Every pipeline that dispatches per-entry work to an in-agent skill
//! (verify, critique, future: mining) follows the same workflow shape:
//!
//! 1. The SDK enqueues per-entry tasks under `.aristo/<pipeline>-queue/`.
//! 2. The in-agent skill spawns N workers; each worker atomically pops
//!    the next pending task via `aristo <pipeline> --pop-next`.
//! 3. The worker computes a result and submits it via
//!    `aristo <pipeline> --submit-<artifact> --id X --json '...'`.
//! 4. The SDK validates the JSON, atomically writes the artifact file,
//!    and removes the corresponding queue entry from `claimed/`.
//! 5. The user (or skill) runs `aristo <pipeline> --apply-X` to validate
//!    every accepted artifact against current index/source and update
//!    the index.
//!
//! This module provides the workflow primitives that every pipeline
//! shares. Per-pipeline `commands/<pipeline>/` modules compose them
//! with their own payload schema, validator, and index-update semantics.
//!
//! The architectural commitments behind this split live in
//! `docs/decisions/critique-and-pipeline-architecture.md`.

pub(crate) mod queue;
