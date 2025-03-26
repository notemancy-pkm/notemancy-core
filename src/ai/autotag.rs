use once_cell::sync::OnceCell;
use rust_bert::pipelines::keywords_extraction::{
    KeywordExtractionConfig, KeywordExtractionModel, KeywordScorerType,
};
use rust_bert::pipelines::sentence_embeddings::{
    SentenceEmbeddingsConfig, SentenceEmbeddingsModelType,
};
use std::error::Error;
use std::sync::{Mutex, MutexGuard};

/// A global instance of the KeywordExtractionModel wrapped in a Mutex for safe reuse.
static KEYWORD_MODEL: OnceCell<Mutex<KeywordExtractionModel>> = OnceCell::new();

/// Lazily initializes and returns a lock guard to the global KeywordExtractionModel.
fn get_keyword_model()
-> Result<MutexGuard<'static, KeywordExtractionModel<'static>>, Box<dyn Error>> {
    KEYWORD_MODEL
        .get_or_try_init(
            || -> Result<Mutex<KeywordExtractionModel>, Box<dyn Error>> {
                // Configure the model to extract keyphrases up to 3 words long.
                let config = KeywordExtractionConfig {
                    sentence_embeddings_config: SentenceEmbeddingsConfig::from(
                        SentenceEmbeddingsModelType::AllMiniLmL6V2,
                    ),
                    scorer_type: KeywordScorerType::MaxSum,
                    ngram_range: (1, 3),
                    num_keywords: 5,
                    ..Default::default()
                };
                let model = KeywordExtractionModel::new(config)?;
                Ok(Mutex::new(model))
            },
        )
        .map_err(|e| {
            Box::<dyn Error>::from(format!("Failed to initialize keyword model: {:?}", e))
        })?
        .lock()
        .map_err(|e| Box::<dyn Error>::from(format!("Failed to acquire model lock: {:?}", e)))
}

/// Generate tags (keywords/keyphrases) for an input text note using a globally initialized rust-bert
/// keyword extraction pipeline.
pub fn generate_tags(text: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let model = get_keyword_model()?;
    let keyword_lists = model.predict(&[text])?;
    let tags = keyword_lists
        .into_iter()
        .next()
        .map(|keywords| keywords.into_iter().map(|k| k.text).collect())
        .unwrap_or_else(Vec::new);
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_generate_tags() -> Result<(), Box<dyn Error>> {
        // Initialize logging to observe download progress (if any).
        let _ = env_logger::builder().is_test(true).try_init();

        let text = "Rust is a multi-paradigm, general-purpose programming language. \
                    Rust emphasizes performance, type safety, and concurrency. \
                    It enforces memory safety without a garbage collector.";
        let tags = generate_tags(text)?;
        assert!(
            !tags.is_empty(),
            "Expected at least one tag to be generated"
        );
        println!("Generated tags: {:?}", tags);
        Ok(())
    }
}
