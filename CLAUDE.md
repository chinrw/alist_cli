# Claude Code Guidelines for alist_cli

### 🔄 Project Awareness & Context

- **Always read `README.md` and `Cargo.toml`** at the start of a new conversation to understand the project's dependencies, features, and configuration.
- **Check existing code structure** before making changes to maintain consistency with current patterns and architecture.
- **Use consistent naming conventions, file structure, and architecture patterns** as established in the existing codebase.
- **Use `cargo` commands** for all Rust operations (build, test, check, clippy, fmt).

### 🧱 Code Structure & Modularity

- **Never create a file longer than 500 lines of code.** If a file approaches this limit, refactor by splitting it into modules or helper files.
- **Organize code into clearly separated modules**, grouped by feature or responsibility:
  - `main.rs` - CLI argument parsing and main application logic
  - `alist_api.rs` - API client and data structures
  - `download.rs` - Download functionality
  - `tracing_bridge.rs` - Logging utilities
- **Use clear, consistent imports** and prefer absolute paths for external crates.
- **Use proper Rust module system** with `mod` declarations and `pub` visibility as needed.

### 🧪 Testing & Reliability

- **Always create unit tests for new features** using Rust's built-in testing framework.
- **After updating any logic**, check whether existing tests need to be updated.
- **Tests should be in the same file as the code** (using `#[cfg(test)]` modules) or in a separate `tests/` directory for integration tests.
- **Include at least:**
  - 1 test for expected behavior
  - 1 edge case test
  - 1 error/failure case test
- **Use `cargo test` to run all tests** and ensure they pass before committing.

### ✅ Code Quality & Standards

- **Always run `cargo check`** before considering code complete.
- **Use `cargo clippy`** to catch common mistakes and improve code quality.
- **Use `cargo fmt`** to maintain consistent code formatting.
- **Handle errors properly** using `Result<T, E>` and avoid `unwrap()` in production code.
- **Use appropriate error types** - prefer `anyhow::Error` for applications, custom errors for libraries.

### 📎 Style & Conventions

- **Use Rust 2024 edition** features and idioms.
- **Follow Rust naming conventions:**
  - `snake_case` for functions, variables, modules
  - `PascalCase` for types, structs, enums
  - `SCREAMING_SNAKE_CASE` for constants
- **Use `serde` for serialization/deserialization**.
- **Use `tokio` for async operations** as established in the project.
- **Write comprehensive documentation** for public APIs:

  ```rust
  /// Brief summary of what the function does.
  ///
  /// # Arguments
  ///
  /// * `param1` - Description of parameter
  /// * `param2` - Description of parameter
  ///
  /// # Returns
  ///
  /// Description of return value
  ///
  /// # Errors
  ///
  /// Description of possible errors
  pub fn example() -> Result<()> {
      // implementation
  }
  ```

### 🔧 Build & Performance

- **Use release profile optimizations** as configured in `Cargo.toml`.
- **Leverage Rust's zero-cost abstractions** - prefer iterators over manual loops.
- **Use appropriate data structures** - `Vec<T>` for owned data, `&[T]` for borrowed slices.
- **Consider memory efficiency** - avoid unnecessary clones, use references where possible.
- **Handle concurrent operations safely** using Rust's ownership system and async/await.

### 📚 Documentation & Maintenance

- **Update `README.md`** when new features are added, dependencies change, or setup steps are modified.
- **Update `Cargo.toml`** when adding new dependencies or changing project metadata.
- **Comment complex algorithms** and business logic with clear explanations.
- **Use `// SAFETY:` comments** when using unsafe code (if any).

### 🧠 AI Behavior Rules

- **Never assume missing context. Ask questions if uncertain.**
- **Never hallucinate crates or functions** – only use known, verified Rust crates from crates.io.
- **Always check `Cargo.toml` dependencies** before using external crates in code.
- **Prefer standard library solutions** when possible, only add dependencies when necessary.
- **Never delete or overwrite existing code** unless explicitly instructed or part of a refactoring task.
- **Always consider error handling** - Rust's type system makes error handling explicit and required.

### 🛠️ Development Workflow

**Development Commands (Always Allowed):**
- **Use `cargo check`** for fast compilation checking during development - run frequently while coding.
- **Use `cargo build --release`** for optimized production builds when ready to test performance.
- **Run `cargo test`** before committing changes to ensure all tests pass.
- **Use `cargo clippy -- -D warnings`** to enforce strict code quality - treat warnings as errors.
- **Use `cargo fmt`** to automatically format code according to Rust standards.
- **Check compilation with different feature flags** if the project uses them.

**Quality Assurance Pipeline:**
1. Run `cargo fmt` to format code
2. Run `cargo check` for quick compilation validation
3. Run `cargo clippy -- -D warnings` for linting (must pass with zero warnings)
4. Run `cargo test` to ensure all tests pass
5. Run `cargo build --release` for final production build verification

**AI Usage:** Claude is always permitted to run these cargo commands to maintain code quality and verify changes.

### 🔒 Security & Safety

- **Leverage Rust's memory safety** - the compiler prevents many classes of bugs.
- **Be cautious with `unsafe` blocks** - document safety requirements thoroughly.
- **Validate external input** using proper parsing and validation.
- **Use secure networking practices** with TLS/HTTPS as configured in the project.
- **Handle sensitive data appropriately** - avoid logging secrets or tokens.

