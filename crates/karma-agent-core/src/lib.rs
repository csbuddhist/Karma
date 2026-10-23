//! Session-agent logic shared by the Windows and macOS agents: inference
//! scheduling and health, OCR model selection, the bundled OCR word pack,
//! frame-to-window attribution, and the agent side of the service protocol.
//! Platform agents add screen capture, window enumeration, and the local IPC
//! transport.

pub mod attribution;
pub mod inference;
pub mod ocr_profile;
pub mod runtime;
pub mod service;
pub mod startup;
