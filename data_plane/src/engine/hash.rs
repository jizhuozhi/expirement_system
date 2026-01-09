use crate::layer::BUCKET_SIZE;
use xxhash_rust::xxh3::xxh3_64;

/// Implementation details
/// Implementation details
pub fn hash_to_bucket(key: &str, salt: &str) -> u32 {
    // Implementation details
    let combined = format!("{}{}", key, salt);
    let hash = xxh3_64(combined.as_bytes());
    (hash % BUCKET_SIZE as u64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_hash_to_bucket_with_salt() {
        let bucket = hash_to_bucket("user_123", "layer1_v1");
        assert!(bucket < BUCKET_SIZE);
    }

    #[test]
    fn test_hash_determinism() {
        let key = "user_456";
        let salt = "experiment_v2";
        let bucket1 = hash_to_bucket(key, salt);
        let bucket2 = hash_to_bucket(key, salt);
        assert_eq!(bucket1, bucket2);
    }

    #[test]
    fn test_different_salts_produce_different_buckets() {
        let key = "user_789";
        let bucket1 = hash_to_bucket(key, "layer1_v1");
        let bucket2 = hash_to_bucket(key, "layer2_v1");

        // Implementation details
        // Implementation details
        assert_ne!(bucket1, bucket2);
    }

    #[test]
    fn test_salt_ensures_different_distribution() {
        let salts = vec!["layer1_v1", "layer2_v1", "layer3_v1"];
        let mut distributions: Vec<HashSet<u32>> = vec![HashSet::new(); salts.len()];

        // Implementation details
        for i in 0..100 {
            let key = format!("user_{}", i);

            for (idx, salt) in salts.iter().enumerate() {
                let bucket = hash_to_bucket(&key, salt);
                distributions[idx].insert(bucket);
            }
        }

        // Implementation details
        for dist in &distributions {
            // Implementation details
            assert!(
                dist.len() >= 95,
                "Expected at least 95 unique buckets, got {}",
                dist.len()
            );
        }
    }

    #[test]
    fn test_hash_distribution() {
        let mut buckets = vec![0; BUCKET_SIZE as usize];
        let salt = "test_layer_v1";

        for i in 0..100000 {
            let key = format!("user_{}", i);
            let bucket = hash_to_bucket(&key, salt);
            buckets[bucket as usize] += 1;
        }

        // Implementation details
        let expected = 100000 / BUCKET_SIZE;
        let mut outside_range = 0;

        for count in buckets.iter() {
            if *count < expected / 2 || *count > expected * 2 {
                outside_range += 1;
            }
        }

        // Implementation details
        assert!(outside_range < BUCKET_SIZE / 20);
    }
}
