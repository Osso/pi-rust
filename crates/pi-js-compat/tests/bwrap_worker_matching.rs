//! Compile the actual worker module independently of the compiler-blocked parent library.
pub mod utils {
    pub mod child_process {
        pub use pi_js_compat::node_fs::node_error_code;
    }
}

#[path = "../../pi-coding-agent/src/bundled_extensions/bwrap/worker.rs"]
pub mod worker;
