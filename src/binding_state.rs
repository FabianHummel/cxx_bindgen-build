#[derive(Debug, Clone, Default)]
pub(crate) struct BindingState {
    pub rust_bindings: Vec<u8>,
    pub shared: Vec<u8>,
}