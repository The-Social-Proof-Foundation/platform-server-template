mod openai;
mod service;

pub use openai::OpenAiEmbeddingClient;
pub use service::EmbeddingService;

pub const EXPECTED_EMBEDDING_DIM: usize = 3072;
