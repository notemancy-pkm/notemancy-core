use once_cell::sync::OnceCell;
use rust_bert::pipelines::sentence_embeddings::{
    SentenceEmbeddingsBuilder, SentenceEmbeddingsModel, SentenceEmbeddingsModelType,
};
use std::error::Error;
use std::sync::{Mutex, MutexGuard};

/// Global static instance to hold the SentenceEmbeddingsModel.
static MODEL: OnceCell<Mutex<SentenceEmbeddingsModel>> = OnceCell::new();

/// Lazily initializes and returns a lock guard to the global SentenceEmbeddingsModel.
fn get_model() -> Result<MutexGuard<'static, SentenceEmbeddingsModel>, Box<dyn Error>> {
    MODEL
        .get_or_try_init(
            || -> Result<Mutex<SentenceEmbeddingsModel>, Box<dyn Error>> {
                let model =
                    SentenceEmbeddingsBuilder::remote(SentenceEmbeddingsModelType::AllMiniLmL6V2)
                        .create_model()?;
                Ok(Mutex::new(model))
            },
        )
        .map_err(|e| Box::<dyn Error>::from(format!("Failed to initialize model: {:?}", e)))?
        .lock()
        .map_err(|e| Box::<dyn Error>::from(format!("Failed to acquire model lock: {:?}", e)))
}

/// Generates sentence embeddings for a single input text by wrapping it in a slice.
pub fn generate_embedding(input_text: &str) -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
    let model = get_model()?;
    let sentences = [input_text];
    let embeddings = model.encode(&sentences)?;
    Ok(embeddings)
}

/// Generates sentence embeddings for a batch of input texts.
pub fn generate_batch_embeddings(input_texts: &[&str]) -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
    let model = get_model()?;
    let embeddings = model.encode(input_texts)?;
    Ok(embeddings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::time::Instant;

    // Helper function to run warm-up calls.
    fn warm_up() -> Result<(), Box<dyn Error>> {
        let _ = generate_embedding("Warm up sentence.")?;
        let _ = generate_batch_embeddings(&["Warm up sentence 1.", "Warm up sentence 2."])?;
        Ok(())
    }

    #[test]
    fn test_timing_comparison() -> Result<(), Box<dyn Error>> {
        // Warm up the model so that initialization overhead is not measured.
        warm_up()?;

        // Prepare a larger batch of sentences.
        let batch_size = 50;
        let sentences: Vec<String> = (0..batch_size)
            .map(|i| format!("This is sentence number {} for timing comparison.", i))
            .collect();
        // Convert to Vec<&str> for batch processing.
        let sentences_ref: Vec<&str> = sentences.iter().map(|s| s.as_str()).collect();

        // Number of iterations for averaging.
        let iterations = 10;

        // Measure batched processing time.
        let mut batched_total: f64 = 0.0;
        for _ in 0..iterations {
            let start = Instant::now();
            let _ = generate_batch_embeddings(&sentences_ref)?;
            batched_total += start.elapsed().as_secs_f64();
        }
        let batched_avg = batched_total / iterations as f64;

        // Measure unbatched processing time by processing each sentence separately.
        let mut unbatched_total: f64 = 0.0;
        for _ in 0..iterations {
            let start = Instant::now();
            for sentence in &sentences {
                let _ = generate_embedding(sentence)?;
            }
            unbatched_total += start.elapsed().as_secs_f64();
        }
        let unbatched_avg = unbatched_total / iterations as f64;

        println!("Batched processing average time: {:.6}s", batched_avg);
        println!("Unbatched processing average time: {:.6}s", unbatched_avg);

        Ok(())
    }
}
