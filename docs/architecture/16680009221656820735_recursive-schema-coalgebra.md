# Recursive Plugin Schema: A Coalgebraic Foundation

## Current State

The plugin schema system is now live with full recursive structure:

```rust
struct PluginSchema {
    namespace: String,
    version: String,
    description: String,
    methods: Vec<MethodSchema>,
    children: Option<Vec<PluginSchema>>,  // None = leaf, Some = hub
}
```

**Category-theoretically**: `Plugin ≅ μX. Methods × (1 + List(X))`

The `schema()` method is the coalgebra structure map:
```
unfold: Plugin → PluginSchema
```

Applied recursively (anamorphism) until all leaves are reached.

## What Was Implemented

### Phase 1: Core Types
- `PluginSchema` with recursive `children` field
- `MethodSchema` with `params` and `returns` JSON schemas
- Helper constructors: `PluginSchema::leaf()`, `PluginSchema::hub()`

### Phase 2: Activation Trait Extension
- Added `plugin_schema()` to `Activation` trait
- Default implementation returns leaf schema

### Phase 3: Hub-Macro Updates
- Added `hub` flag to `#[hub_methods]` attribute
- When `hub = true`, generates `plugin_schema()` that calls `self.plugin_children()`
- Updated to use `MethodSchema` instead of removed `MethodSchemaInfo`

### Phase 4: Plexus as Hub
- Plexus uses `#[hub_methods(..., hub)]`
- Implements `plugin_children()` returning child schemas
- Schema RPC now returns recursive `PluginSchema`

### Phase 5: Cleanup
- Removed session types (`method.rs`, `session_schema.rs`, `typed_methods.rs`)
- Deprecated `ActivationFullSchema` (now alias to `PluginSchema`)

## Challenges Encountered

### 1. Naming Collision
The existing `MethodSchema` in `method.rs` was for session-typed protocols (dialectic). Our new `MethodSchema` is simpler (just params/returns).

**Solution**: Deleted session types entirely. They were experimental and unused.

### 2. Hub-Macro Override
The macro generates `impl Activation` which includes `plugin_schema()`. Plexus needs a different implementation (one that includes children).

**Solution**: Added `hub` flag to the macro. When set, the generated `plugin_schema()` calls `self.plugin_children()` which the struct must implement.

### 3. Cargo Path Issues
Worktree configuration had stale paths to hub-macro.

**Solution**: Fixed path in Cargo.toml to `../../../../hypermemetic/hub-macro`.

## Shortcuts Taken

1. **No streaming schema traversal** - Schema is returned as single JSON blob. For very large plugin trees, streaming would be better.

2. **No schema caching** - `plugin_schema()` rebuilds on every call. Should cache and invalidate on registration changes.

3. **No versioning validation** - Children don't validate version compatibility with parents.

4. **Synchronous children** - `plugin_children()` is sync. Async would allow lazy/remote plugin discovery.

---

## The CLI as a Catamorphism

The schema is an **unfold** (anamorphism) — tearing down plugins into observable structure.

The CLI is a **fold** (catamorphism) — building up a parser/dispatcher from that structure.

```
                    schema (unfold)
            Plugin ──────────────────→ PluginSchema
                                            │
                                            │ CLI (fold)
                                            ▼
                                       CliParser
```

These compose: `cli = fold ∘ unfold` — a **hylomorphism**.

## The Navigation Category

Objects are *positions* in the schema tree:

```
Root → Plugin → Method → Invocation
```

Morphisms are *user inputs* that transition between positions:

| Position | User Input | Transition |
|----------|-----------|------------|
| Root | `<plugin>` | → Plugin |
| Plugin | `<method>` | → Method |
| Plugin | `<child>` | → Plugin (nested hub) |
| Method | `--params` | → Invocation |

## The Result Type (Coproduct)

Invocation yields one of:

```
Result = SchemaView        -- help/discovery output
       + ParamNameError    -- unknown param (caught by CLI)
       + ParamValueError   -- invalid value for type (caught by CLI)
       + RuntimeError      -- plugin execution failed
       + Output            -- success, stream the response
```

The first two are *preventable* — the CLI has enough information from the schema to reject before calling the system.

## State Machine View (Coalgebra Again!)

The CLI itself is a coalgebra:

```rust
enum CliState {
    AtRoot,
    AtPlugin { path: Vec<String>, schema: PluginSchema },
    AtMethod { path: Vec<String>, method: MethodSchema },
    Invoking { path: Vec<String>, method: String, params: Value },
}

// Coalgebra structure map
fn step(state: CliState, input: &str) -> CliState + Output + Error
```

Each state observes differently:
- `AtRoot` → list children
- `AtPlugin` → list methods + children
- `AtMethod` → show param schema
- `Invoking` → validate, call, stream

## Sketch Implementation

```rust
struct Cli {
    root_schema: PluginSchema,
}

impl Cli {
    /// Navigate to a position, return the view
    fn resolve(&self, path: &[String]) -> Result<SchemaView, NavigationError> {
        let mut current = &self.root_schema;

        for segment in path {
            // Try as child plugin first
            if let Some(children) = &current.children {
                if let Some(child) = children.iter().find(|c| c.namespace == *segment) {
                    current = child;
                    continue;
                }
            }
            // Then as method
            if current.methods.iter().any(|m| m.name == *segment) {
                return Ok(SchemaView::Method(/* ... */));
            }
            return Err(NavigationError::NotFound(segment.clone()));
        }

        Ok(SchemaView::Plugin(current.clone()))
    }

    /// Validate params against schema before invocation
    fn validate_params(&self, method: &MethodSchema, params: &Value) -> Result<(), ParamError> {
        if let Some(schema) = &method.params {
            // JSON Schema validation here
            jsonschema::validate(schema, params)?;
        }
        Ok(())
    }

    /// The main dispatch
    fn execute(&self, args: Args) -> Result<Output, CliError> {
        match self.resolve(&args.path)? {
            SchemaView::Plugin(p) => Ok(Output::Help(render_plugin_help(&p))),
            SchemaView::Method(m) => {
                if args.params.is_empty() {
                    Ok(Output::Help(render_method_help(&m)))
                } else {
                    self.validate_params(&m, &args.params)?;
                    self.invoke(&args.path, &args.params).await
                }
            }
        }
    }
}
```

## The Help Rendering Functor

```
render: PluginSchema → String
```

This is a natural transformation from the schema functor to the "display" functor:

```rust
fn render_plugin_help(schema: &PluginSchema) -> String {
    let mut out = format!("{} v{}\n\n", schema.namespace, schema.version);
    out += &schema.description;
    out += "\n\nMethods:\n";
    for m in &schema.methods {
        out += &format!("  {}  {}\n", m.name, m.description);
    }
    if let Some(children) = &schema.children {
        out += "\nPlugins:\n";
        for c in children {
            out += &format!("  {}  {}\n", c.namespace, c.description);
        }
    }
    out
}
```

## Example Session

```bash
$ plugin-cli
plexus v1.0.0

Central routing and introspection

Methods:
  schema    Get this plugin's schema
  call      Route a call to a plugin

Plugins:
  echo      Echo service
  health    Health checks

$ plugin-cli echo
echo v1.0.0

Echo service

Methods:
  echo      Echo a message back
  once      Echo once

$ plugin-cli echo echo
echo.echo

Echo a message back

Params:
  --message <string>  The message to echo (required)
  --count <uint32>    Number of times to repeat (required)

$ plugin-cli echo echo --message "hello" --count 2
{"event": "echo", "message": "hello", "count": 1}
{"event": "echo", "message": "hello", "count": 2}

$ plugin-cli echo echo --badparam "hello"
Error: Unknown parameter '--badparam'

Valid params: --message <string>, --count <uint32>

$ plugin-cli echo echo --message 123
Error: Invalid value for '--message': expected string, got number
```

## The Categorical Punchline

The system now has:

1. **Schema coalgebra** — plugins unfold into observable structure
2. **CLI algebra** — structure folds into a parser/dispatcher
3. **Hylomorphism** — the composition `parse ∘ schema` builds CLI from plugins
4. **Error coproduct** — typed failure modes, some preventable by schema validation
5. **State coalgebra** — the CLI itself is a state machine with observable transitions

The CLI doesn't hardcode commands — it *derives* them from the category of plugins. Add a plugin, get a CLI command for free.

---

## Future Improvements

### 1. Schema Streaming (Priority: Medium)
For large plugin trees, stream schema traversal instead of materializing the whole tree:

```rust
async fn stream_schema(&self) -> impl Stream<Item = SchemaNode> {
    // Breadth-first traversal yielding nodes
}
```

### 2. Schema Caching (Priority: High)
Cache `plugin_schema()` results and invalidate on registration:

```rust
struct Plexus {
    schema_cache: RwLock<Option<PluginSchema>>,
}

fn register<A: Activation>(mut self, activation: A) -> Self {
    // ... registration ...
    *self.schema_cache.write() = None;  // Invalidate
    self
}
```

### 3. Lazy/Remote Plugin Discovery (Priority: Low)
Allow `plugin_children()` to be async for remote plugin registries:

```rust
#[async_trait]
trait Activation {
    async fn plugin_children(&self) -> Vec<PluginSchema> {
        vec![]  // Default: no children
    }
}
```

### 4. Schema Versioning (Priority: Medium)
Add semantic version constraints:

```rust
struct PluginSchema {
    requires: Vec<VersionConstraint>,  // e.g., "health >= 1.0.0"
}
```

### 5. Interactive REPL Mode (Priority: High)
The state machine coalgebra naturally supports a REPL:

```rust
loop {
    let view = cli.render_current_state();
    print!("{}\n> ", view);

    let input = read_line();
    match cli.step(input) {
        Transition::Navigate(new_state) => cli.state = new_state,
        Transition::Output(stream) => stream.for_each(print).await,
        Transition::Error(e) => eprintln!("{}", e),
    }
}
```

### 6. Schema Diffing (Priority: Low)
Track schema changes for cache invalidation and client notification:

```rust
fn schema_diff(old: &PluginSchema, new: &PluginSchema) -> SchemaDiff {
    // Added/removed/changed methods and children
}
```

---

## Validation: Live Output

```json
{
  "namespace": "plexus",
  "version": "1.0.0",
  "description": "Central routing and introspection",
  "methods": [
    {"name": "call", "description": "Route a call to a registered activation", ...},
    {"name": "hash", "description": "Get plexus configuration hash", ...},
    {"name": "list_activations", ...},
    {"name": "schema", "description": "Get full plexus schema (recursive, includes all children)", ...}
  ],
  "children": [
    {
      "namespace": "health",
      "version": "1.0.0",
      "methods": [{"name": "check", ...}]
    },
    {
      "namespace": "echo",
      "version": "1.0.0",
      "methods": [
        {"name": "echo", "params": {...}, "returns": {...}},
        {"name": "once", "params": {...}, "returns": {...}}
      ]
    }
  ]
}
```

The foundation is laid. The CLI interpreter is the natural next step.
