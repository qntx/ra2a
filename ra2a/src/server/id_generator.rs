//! ID generation traits and implementations.
//!
//! Provides pluggable ID generation for tasks and contexts.

use uuid::Uuid;

/// Context for ID generation, providing hints for ID creation.
#[derive(Debug, Clone, Default)]
pub struct IDGeneratorContext {
    /// An existing task ID that might influence generation.
    pub task_id: Option<String>,
    /// An existing context ID that might influence generation.
    pub context_id: Option<String>,
    /// A prefix to use for the generated ID.
    pub prefix: Option<String>,
}

impl IDGeneratorContext {
    /// Creates a new empty generator context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a context with a task ID hint.
    pub fn with_task_id(task_id: impl Into<String>) -> Self {
        Self {
            task_id: Some(task_id.into()),
            ..Default::default()
        }
    }

    /// Creates a context with a context ID hint.
    pub fn with_context_id(context_id: impl Into<String>) -> Self {
        Self {
            context_id: Some(context_id.into()),
            ..Default::default()
        }
    }

    /// Sets a prefix for the generated ID.
    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }
}

/// Trait for generating unique identifiers.
///
/// Implementations can provide different strategies for ID generation,
/// such as UUIDs, sequential IDs, or custom formats.
pub trait IDGenerator: Send + Sync {
    /// Generates a new unique identifier.
    fn generate(&self, context: &IDGeneratorContext) -> String;
}

/// UUID-based ID generator (default implementation).
#[derive(Debug, Clone, Default)]
pub struct UUIDGenerator {
    /// Optional prefix to prepend to generated IDs.
    prefix: Option<String>,
}

impl UUIDGenerator {
    /// Creates a new UUID generator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new UUID generator with a prefix.
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: Some(prefix.into()),
        }
    }
}

impl IDGenerator for UUIDGenerator {
    fn generate(&self, context: &IDGeneratorContext) -> String {
        let uuid = Uuid::new_v4().to_string();

        // Use context prefix if set, otherwise use generator's prefix
        match context.prefix.as_ref().or(self.prefix.as_ref()) {
            Some(prefix) => format!("{}-{}", prefix, uuid),
            None => uuid,
        }
    }
}

/// Sequential ID generator for testing or special use cases.
#[derive(Debug)]
pub struct SequentialGenerator {
    counter: std::sync::atomic::AtomicU64,
    prefix: String,
}

impl SequentialGenerator {
    /// Creates a new sequential generator with the given prefix.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            counter: std::sync::atomic::AtomicU64::new(1),
            prefix: prefix.into(),
        }
    }

    /// Creates a new sequential generator starting from a specific value.
    pub fn starting_at(prefix: impl Into<String>, start: u64) -> Self {
        Self {
            counter: std::sync::atomic::AtomicU64::new(start),
            prefix: prefix.into(),
        }
    }
}

impl IDGenerator for SequentialGenerator {
    fn generate(&self, _context: &IDGeneratorContext) -> String {
        let id = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("{}-{}", self.prefix, id)
    }
}

/// Composite generator that uses different generators for tasks and contexts.
#[derive(Debug)]
pub struct CompositeGenerator<T: IDGenerator, C: IDGenerator> {
    task_generator: T,
    context_generator: C,
}

impl<T: IDGenerator, C: IDGenerator> CompositeGenerator<T, C> {
    /// Creates a new composite generator.
    pub fn new(task_generator: T, context_generator: C) -> Self {
        Self {
            task_generator,
            context_generator,
        }
    }

    /// Generates a task ID.
    pub fn generate_task_id(&self, context: &IDGeneratorContext) -> String {
        self.task_generator.generate(context)
    }

    /// Generates a context ID.
    pub fn generate_context_id(&self, context: &IDGeneratorContext) -> String {
        self.context_generator.generate(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uuid_generator() {
        let generator = UUIDGenerator::new();
        let ctx = IDGeneratorContext::new();

        let id1 = generator.generate(&ctx);
        let id2 = generator.generate(&ctx);

        assert_ne!(id1, id2);
        assert_eq!(id1.len(), 36); // UUID v4 format
    }

    #[test]
    fn test_uuid_generator_with_prefix() {
        let generator = UUIDGenerator::with_prefix("task");
        let ctx = IDGeneratorContext::new();

        let id = generator.generate(&ctx);
        assert!(id.starts_with("task-"));
    }

    #[test]
    fn test_context_prefix_overrides() {
        let generator = UUIDGenerator::with_prefix("default");
        let ctx = IDGeneratorContext::new().prefix("override");

        let id = generator.generate(&ctx);
        assert!(id.starts_with("override-"));
    }

    #[test]
    fn test_sequential_generator() {
        let generator = SequentialGenerator::new("seq");
        let ctx = IDGeneratorContext::new();

        assert_eq!(generator.generate(&ctx), "seq-1");
        assert_eq!(generator.generate(&ctx), "seq-2");
        assert_eq!(generator.generate(&ctx), "seq-3");
    }

    #[test]
    fn test_sequential_generator_starting_at() {
        let generator = SequentialGenerator::starting_at("test", 100);
        let ctx = IDGeneratorContext::new();

        assert_eq!(generator.generate(&ctx), "test-100");
        assert_eq!(generator.generate(&ctx), "test-101");
    }

    #[test]
    fn test_composite_generator() {
        let generator = CompositeGenerator::new(
            UUIDGenerator::with_prefix("task"),
            UUIDGenerator::with_prefix("ctx"),
        );
        let ctx = IDGeneratorContext::new();

        let task_id = generator.generate_task_id(&ctx);
        let context_id = generator.generate_context_id(&ctx);

        assert!(task_id.starts_with("task-"));
        assert!(context_id.starts_with("ctx-"));
    }
}
