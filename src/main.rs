use std::env;
use std::sync::{Arc, Barrier};
use std::sync::atomic::{AtomicU64, Ordering};
use std::collections::{HashMap, HashSet};
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use rayon::prelude::*;
use std::time::Instant;
use tokio::fs::File as TokioFile;
use tokio::io::{self, AsyncBufReadExt, BufReader};

#[tokio::main]
async fn main() {
    // Set the number of threads for Rayon
    env::set_var("RAYON_NUM_THREADS", "8"); // Set to desired number of threads

    // Set the chunk size for processing words
    let chunk_size = 100000;

    let m = MultiProgress::new();
    let start_total = Instant::now();

    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        println!("Usage: cargo run -- <hashlist.txt> <wordlist.txt>");
        return;
    }

    let hashlist_path = &args[1];
    let wordlist_path = &args[2];

    let hashlist_task = read_hashlist_parallel(hashlist_path);
    let wordlist_task = read_wordlist_parallel(wordlist_path);
    let (hashes_result, words_result) = tokio::join!(hashlist_task, wordlist_task);
    let hashes = match hashes_result {
        Ok(hashes) => Arc::new(hashes),
        Err(err) => {
            eprintln!("Failed to read hashlist: {}", err);
            return;
        }
    };
    let words = match words_result {
        Ok(words) => Arc::new(words),
        Err(err) => {
            eprintln!("Failed to read wordlist: {}", err);
            return;
        }
    };

    let progress_bar = m.add(ProgressBar::new(words.len() as u64));
    progress_bar.set_style(ProgressStyle::default_bar()
        .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta_precise}) {msg}")
        .progress_chars("#>-"));

    let hash_count = Arc::new(AtomicU64::new(0));
    let hash_map = Arc::new(index_hashes(&hashes, 2));

    // barrier to synchronize progress bar
    let barrier = Arc::new(Barrier::new(2));
    let barrier_clone = Arc::clone(&barrier); // Clone barrier

    let m_clone = Arc::clone(&Arc::new(m)); // Cloning the Arc
    let progress_draw_thread = std::thread::spawn(move || {
        m_clone.join_and_clear().expect("Failed to join MultiProgress");
        barrier_clone.wait();
    });
    
    let start_processing = Instant::now(); // Start timer 
    
    // words in chunks
    words.par_chunks(chunk_size).for_each_with(progress_bar.clone(), |pb, word_chunk| {
        for word in word_chunk {
            let computed_hash = md5_hash(word.as_bytes());
            if let Some(hashes) = hash_map.get(&computed_hash[0..2]) {
                if hashes.contains(&computed_hash) {
                }
            }
        }
        // atomic operation
        pb.inc(word_chunk.len() as u64);
        hash_count.fetch_add(word_chunk.len() as u64, Ordering::Relaxed);
    });

    let duration_processing = start_processing.elapsed(); // Stop timer

    progress_bar.finish_with_message("Hashing complete");

    barrier.wait();

    let duration_total = start_total.elapsed();
    let total_hashes = hash_count.load(Ordering::Relaxed);
    
    println!("Total time: {:?}", duration_total);
    println!("Total processing time: {:?}", duration_processing);
    println!("Total hashes: {}, Hash rate: {}", total_hashes, format_hash_rate(total_hashes as f64 / duration_processing.as_secs_f64()));

    progress_draw_thread.join().unwrap();
}

async fn read_hashlist_parallel(filename: &str) -> Result<Vec<String>, io::Error> {
    let file = TokioFile::open(filename).await?;
    let reader = BufReader::new(file);
    let mut lines = Vec::new();
    let mut stream = reader.lines();
    while let Some(line) = stream.next_line().await? {
        lines.push(line);
    }
    Ok(lines)
}

async fn read_wordlist_parallel(filename: &str) -> Result<Vec<String>, io::Error> {
    let file = TokioFile::open(filename).await?;
    let reader = BufReader::new(file);
    let mut lines = Vec::new();
    let mut stream = reader.lines();
    while let Some(line) = stream.next_line().await? {
        lines.push(line);
    }
    Ok(lines)
}

fn index_hashes(hashes: &[String], prefix_len: usize) -> HashMap<String, HashSet<String>> {
    let mut hash_map = HashMap::new();
    for hash in hashes {
        let entry = hash_map.entry(hash[..prefix_len].to_string()).or_insert_with(HashSet::new);
        entry.insert(hash.clone());
    }
    hash_map
}

fn md5_hash(key: &[u8]) -> String {
    let digest = md5::compute(key);
    format!("{:x}", digest)
}

fn format_hash_rate(rate: f64) -> String {
    if rate >= 1_000_000_000.0 {
        format!("{:.2} GH/s", rate / 1_000_000_000.0)
    } else if rate >= 1_000_000.0 {
        format!("{:.2} MH/s", rate / 1_000_000.0)
    } else if rate >= 1_000.0 {
        format!("{:.2} KH/s", rate / 1_000.0)
    } else {
        format!("{:.2} H/s", rate)
    }
}
