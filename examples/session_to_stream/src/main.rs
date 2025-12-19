//! Session Types to Stream Mapping
//!
//! This example demonstrates how session types (the protocol contract) map to
//! streams (the wire format). Over the wire, we only have streams of messages.
//! The session type is an abstraction that describes what messages can be sent
//! when, and the stream is the concrete realization.
//!
//! Key insight: A session type describes a protocol. A stream is a sequence of
//! messages. The session type constrains what the stream can contain.

// Import session types with a prefix to avoid shadowing std::marker::Send
use dialectic::types::{Choose, Continue, Done, Loop, Offer, Recv, Send as SessionSend};
use futures::{Stream, StreamExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

// ============================================================================
// Part 1: Define the Protocol Types
// ============================================================================

/// Events that can be sent during bash execution
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BashEvent {
    Stdout { line: String },
    Stderr { line: String },
    Exit { code: i32 },
}

/// The session type for bash execution (server perspective):
/// - Receive the command
/// - Loop: Choose to send stdout/stderr (continue) or exit (break)
pub type BashServerProtocol = Recv<String, Loop<Choose<(SessionSend<BashEvent, Continue<0>>, SessionSend<BashEvent, Done>)>>>;

/// The dual (client perspective):
/// - Send the command
/// - Loop: Offer to receive event (continue) or final event (break)
pub type BashClientProtocol = SessionSend<String, Loop<Offer<(Recv<BashEvent, Continue<0>>, Recv<BashEvent, Done>)>>>;

// ============================================================================
// Part 2: The Wire Format - Tagged Stream Messages
// ============================================================================

/// A wire message wraps the payload with metadata about the protocol state.
/// This is what actually gets sent over the network.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "wire_type", rename_all = "snake_case")]
pub enum WireMessage<T> {
    /// A data message (corresponds to Send in session type)
    Data { payload: T },
    /// Protocol continuation marker (corresponds to Continue)
    Continue,
    /// Protocol completion marker (corresponds to Done)
    Done,
    /// Choice indicator (corresponds to Choose/Offer branch selection)
    Branch { index: usize },
}

// ============================================================================
// Part 3: Session Type Interpretation
// ============================================================================

/// Trait for types that can be "executed" as a stream producer.
///
/// The session type describes WHAT can be sent.
/// This trait describes HOW to produce a stream that conforms to the session type.
pub trait SessionExecutor {
    /// The type of items this session produces
    type Item;

    /// Execute the session, producing a stream of wire messages
    fn execute<F, Fut>(
        producer: F,
    ) -> Pin<Box<dyn Stream<Item = WireMessage<Self::Item>> + Send>>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Pin<Box<dyn Stream<Item = Self::Item> + Send>>> + Send + 'static;
}

// ============================================================================
// Part 4: Concrete Implementation - Bash Execution
// ============================================================================

/// Execute a bash-like command, producing a stream that conforms to BashServerProtocol.
///
/// The session type says:
/// - Loop { Choose { Send<Event> + Continue, Send<Event> + Done } }
///
/// This maps to a stream of:
/// - WireMessage::Branch { index: 0 } + WireMessage::Data { event } (for each output line)
/// - WireMessage::Branch { index: 1 } + WireMessage::Data { exit_event } (at the end)
pub fn execute_bash(command: &str) -> impl Stream<Item = WireMessage<BashEvent>> {
    let command = command.to_string();

    async_stream::stream! {
        // Simulate bash execution
        println!("Executing: {}", command);

        // Emit some stdout
        for i in 1..=3 {
            // Branch 0 = continue with more data
            yield WireMessage::Branch { index: 0 };
            yield WireMessage::Data {
                payload: BashEvent::Stdout {
                    line: format!("Output line {}", i)
                }
            };
        }

        // Final event - branch 1 = done
        yield WireMessage::Branch { index: 1 };
        yield WireMessage::Data {
            payload: BashEvent::Exit { code: 0 }
        };
    }
}

/// A simpler approach: just produce the events, infer the protocol structure.
/// The session type tells us the structure, but the stream just contains the data.
pub fn execute_bash_simple(command: &str) -> impl Stream<Item = BashEvent> {
    let command = command.to_string();

    async_stream::stream! {
        println!("Executing: {}", command);

        for i in 1..=3 {
            yield BashEvent::Stdout { line: format!("Output line {}", i) };
        }

        yield BashEvent::Exit { code: 0 };
    }
}

// ============================================================================
// Part 5: Stream to Session Type Validation
// ============================================================================

/// Given a stream and a session type, we can validate that the stream
/// conforms to the protocol. This is useful for:
/// - Testing that implementations are correct
/// - Runtime validation of incoming streams from untrusted sources
///
/// For now, this is conceptual - the key insight is that:
/// - Session type = compile-time contract
/// - Stream validation = runtime verification that stream follows contract

// ============================================================================
// Part 6: The Key Insight - Mapping Patterns
// ============================================================================

/// Common session type patterns and their stream representations:
///
/// 1. Simple request-response:
///    Session: Recv<Request, Send<Response, Done>>
///    Stream:  Single Response item
///
/// 2. Server streaming:
///    Session: Recv<Request, Loop<Choose<(Send<Item, Continue>, Send<Final, Done>)>>>
///    Stream:  Multiple Item, then Final
///
/// 3. Client streaming:
///    Session: Loop<Offer<(Recv<Item, Continue>, Recv<Final, Done>)>> then Send<Response, Done>
///    Stream:  Receive multiple Item, then Final, then send Response
///
/// 4. Bidirectional:
///    Session: Loop<Choose/Offer combinations>
///    Stream:  Interleaved sends and receives
///
/// For RPC subscriptions (jsonrpsee), we primarily use pattern #2:
/// The server receives a request and streams responses until done.

#[tokio::main]
async fn main() {
    println!("=== Session Type to Stream Mapping ===\n");

    // Approach 1: Wire messages with explicit protocol markers
    println!("--- Approach 1: Explicit Wire Messages ---");
    let stream = execute_bash("ls -la");
    tokio::pin!(stream);

    while let Some(msg) = stream.next().await {
        println!("Wire: {:?}", msg);
    }

    println!("\n--- Approach 2: Simple Event Stream ---");
    // Approach 2: Just the events, protocol is implicit
    let stream = execute_bash_simple("echo hello");
    tokio::pin!(stream);

    while let Some(event) = stream.next().await {
        println!("Event: {:?}", event);
    }

    println!("\n--- Approach 3: Session-Typed Stream (THE ANSWER) ---");
    // The session type defines the enum, the stream yields enum variants
    let stream = execute_typed("cat file.txt");
    tokio::pin!(stream);

    while let Some(item) = stream.next().await {
        let terminal = if item.is_terminal() { " [TERMINAL]" } else { "" };
        println!("Item: {:?}{}", item, terminal);
    }

    println!("\n=== Key Takeaways ===");
    println!("1. Session type = protocol contract (what CAN be sent)");
    println!("2. Stream = wire format (what IS sent)");
    println!("3. Session type GENERATES the stream item enum");
    println!("4. Each Choose branch -> one enum variant");
    println!("5. Continue -> non-terminal, Done -> terminal variant");
    println!("6. Stream<Item = GeneratedEnum> implements the session!");
}

// ============================================================================
// Part 7: What This Means for hub-macro
// ============================================================================

// ============================================================================
// Part 8: Session Type -> Stream Item Enum (THE KEY INSIGHT)
// ============================================================================

/// A session type can be mechanically converted to a stream item enum!
///
/// The session type:
///   Loop<Choose<(Send<StdoutEvent, Continue<0>>, Send<ExitEvent, Done>)>>
///
/// Maps to this enum:
///   enum StreamItem {
///       Stdout(StdoutEvent),  // Continue branch
///       Exit(ExitEvent),      // Done branch (terminal)
///   }
///
/// The enum variants correspond 1:1 with the Choose branches.
/// The "Continue" vs "Done" distinction becomes "is this the terminal variant?"

/// Stream item derived from session type:
/// Loop<Choose<(Send<Stdout, Continue>, Send<Stderr, Continue>, Send<Exit, Done>)>>
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BashStreamItem {
    /// Corresponds to: Send<StdoutEvent, Continue<0>>
    Stdout(StdoutEvent),
    /// Corresponds to: Send<StderrEvent, Continue<0>>
    Stderr(StderrEvent),
    /// Corresponds to: Send<ExitEvent, Done> - TERMINAL
    Exit(ExitEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StdoutEvent {
    pub line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StderrEvent {
    pub line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExitEvent {
    pub code: i32,
}

/// Trait that session-derived enums implement
pub trait SessionItem: Sized {
    /// Is this the terminal variant? (corresponds to Done in session type)
    fn is_terminal(&self) -> bool;
}

impl SessionItem for BashStreamItem {
    fn is_terminal(&self) -> bool {
        matches!(self, BashStreamItem::Exit(_))
    }
}

/// The session type for this stream:
/// After receiving input, loop sending items until Exit
pub type BashStreamProtocol = Recv<
    String,
    Loop<Choose<(
        SessionSend<BashStreamItem, Continue<0>>,  // Stdout/Stderr -> continue
        SessionSend<BashStreamItem, Done>,          // Exit -> done
    )>>
>;

/// Execute bash and produce a typed stream
pub fn execute_typed(command: &str) -> impl Stream<Item = BashStreamItem> {
    let command = command.to_string();

    async_stream::stream! {
        // Simulate execution
        yield BashStreamItem::Stdout(StdoutEvent { line: format!("$ {}", command) });
        yield BashStreamItem::Stdout(StdoutEvent { line: "output line 1".to_string() });
        yield BashStreamItem::Stderr(StderrEvent { line: "warning: something".to_string() });
        yield BashStreamItem::Stdout(StdoutEvent { line: "output line 2".to_string() });
        yield BashStreamItem::Exit(ExitEvent { code: 0 });
    }
}

// ============================================================================
// Part 9: The Macro Could Generate This!
// ============================================================================

// Given a session type, we can derive the stream item enum:
//
// Input session type:
//   Loop<Choose<(
//       Send<StdoutEvent, Continue<0>>,
//       Send<StderrEvent, Continue<0>>,
//       Send<ExitEvent, Done>,
//   )>>
//
// Generated enum:
//   #[derive(SessionItem)]
//   enum GeneratedStreamItem {
//       #[session(continue)]
//       Stdout(StdoutEvent),
//       #[session(continue)]
//       Stderr(StderrEvent),
//       #[session(done)]
//       Exit(ExitEvent),
//   }
//
// The macro walks the session type and extracts:
// - Each Send<T, _> becomes a variant containing T
// - Continue<_> marks non-terminal variants
// - Done marks the terminal variant

// ============================================================================
// Part 10: Extracting Types from Session Types
// ============================================================================

/// We can use Rust's type system to extract info from session types!
/// This trait extracts the "sent type" from a Send session type.

pub trait ExtractSendType {
    type Payload;
    type Continuation;
}

impl<T, S> ExtractSendType for SessionSend<T, S> {
    type Payload = T;
    type Continuation = S;
}

/// Check if a session type is terminal
pub trait IsTerminal {
    const IS_TERMINAL: bool;
}

impl IsTerminal for Done {
    const IS_TERMINAL: bool = true;
}

impl<const N: usize> IsTerminal for Continue<N> {
    const IS_TERMINAL: bool = false;
}

impl<T, S: IsTerminal> IsTerminal for SessionSend<T, S> {
    const IS_TERMINAL: bool = S::IS_TERMINAL;
}

// ============================================================================
// Part 11: Full Circle - Session Type IS the Stream Definition
// ============================================================================

// The beautiful conclusion:
//
// 1. Define your protocol as a session type
// 2. The session type GENERATES the stream item enum
// 3. Implement your method returning Stream<Item = GeneratedEnum>
// 4. The session type provides the schema
// 5. The stream provides the runtime
//
// Example:
//
// ```
// // This session type:
// type Protocol = Loop<Choose<(
//     Send<Chunk, Continue<0>>,
//     Send<Complete, Done>,
// )>>;
//
// // Generates this enum:
// enum ProtocolItem {
//     Chunk(Chunk),      // from Send<Chunk, Continue>
//     Complete(Complete), // from Send<Complete, Done>
// }
//
// // And you implement:
// async fn my_method(&self) -> impl Stream<Item = ProtocolItem> { ... }
// ```
//
// The session type and stream are now unified!

// ============================================================================
// Part 12: How a Macro Would Generate the Enum
// ============================================================================

// The macro could work like this:
//
// INPUT:
// ```
// #[hub_method]
// async fn execute(&self, command: String)
//     -> Loop<Choose<(
//            Send<StdoutEvent, Continue<0>>,
//            Send<StderrEvent, Continue<0>>,
//            Send<ExitEvent, Done>,
//        )>>
// {
//     todo!()
// }
// ```
//
// OUTPUT (generated):
// ```
// // The enum derived from the session type
// #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
// #[serde(tag = "type", rename_all = "snake_case")]
// pub enum ExecuteStreamItem {
//     Stdout(StdoutEvent),
//     Stderr(StderrEvent),
//     #[serde(rename = "exit")]
//     Exit(ExitEvent),
// }
//
// impl SessionItem for ExecuteStreamItem {
//     fn is_terminal(&self) -> bool {
//         matches!(self, Self::Exit(_))
//     }
// }
//
// // The actual method signature for RPC
// async fn execute(&self, command: String) -> impl Stream<Item = ExecuteStreamItem>;
//
// // Schema extraction still works from the session type
// fn execute_schema() -> MethodSchema { ... }
// ```
//
// The key transformation:
// - Parse the session type AST
// - Extract each Send<T, _> in the Choose tuple
// - Generate enum variant for each T
// - Mark variants with Done continuation as terminal
// - Generate SessionItem impl
// - Rewrite return type to Stream<Item = GeneratedEnum>

// ============================================================================
// Part 13: Alternative - User Defines Enum, Derives Session Type
// ============================================================================

// We could also go the OTHER direction:
// User defines the enum with attributes, we derive the session type!
//
// ```
// #[derive(SessionType)]
// #[serde(tag = "type", rename_all = "snake_case")]
// pub enum BashStreamItem {
//     #[session(continue)]
//     Stdout(StdoutEvent),
//
//     #[session(continue)]
//     Stderr(StderrEvent),
//
//     #[session(done)]  // Terminal!
//     Exit(ExitEvent),
// }
//
// // This generates:
// impl BashStreamItem {
//     pub type Protocol = Loop<Choose<(
//         Send<StdoutEvent, Continue<0>>,
//         Send<StderrEvent, Continue<0>>,
//         Send<ExitEvent, Done>,
//     )>>;
// }
// ```
//
// This might be MORE ergonomic because:
// 1. Users already think in terms of "what events can I send"
// 2. The enum is the natural way to represent variants
// 3. Session types are complex to write correctly
// 4. #[session(done)] is clearer than Done in a type position

// ============================================================================
// Part 14: Extracting Full Schema from Session Types
// ============================================================================

/// We can define a trait that extracts schema info from any session type!
/// This works at compile time using Rust's trait system.

use std::any::TypeId;

/// Schema representation of a protocol step
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProtocolSchema {
    Send {
        payload_type: String,
        payload_schema: serde_json::Value,
        then: Box<ProtocolSchema>,
    },
    Recv {
        payload_type: String,
        payload_schema: serde_json::Value,
        then: Box<ProtocolSchema>,
    },
    Choose {
        branches: Vec<ProtocolSchema>,
    },
    Offer {
        branches: Vec<ProtocolSchema>,
    },
    Loop {
        body: Box<ProtocolSchema>,
    },
    Continue {
        index: usize,
    },
    Done,
}

/// Trait that session types implement to provide their schema
pub trait ToProtocolSchema {
    fn protocol_schema() -> ProtocolSchema;
}

// Implement for Done
impl ToProtocolSchema for Done {
    fn protocol_schema() -> ProtocolSchema {
        ProtocolSchema::Done
    }
}

// Implement for Continue
impl<const N: usize> ToProtocolSchema for Continue<N> {
    fn protocol_schema() -> ProtocolSchema {
        ProtocolSchema::Continue { index: N }
    }
}

// Implement for Send<T, S> where T: JsonSchema and S: ToProtocolSchema
impl<T, S> ToProtocolSchema for SessionSend<T, S>
where
    T: JsonSchema + 'static,
    S: ToProtocolSchema,
{
    fn protocol_schema() -> ProtocolSchema {
        let schema = schemars::schema_for!(T);
        ProtocolSchema::Send {
            payload_type: std::any::type_name::<T>().to_string(),
            payload_schema: serde_json::to_value(&schema).unwrap_or(serde_json::Value::Null),
            then: Box::new(S::protocol_schema()),
        }
    }
}

// Implement for Recv<T, S>
impl<T, S> ToProtocolSchema for Recv<T, S>
where
    T: JsonSchema + 'static,
    S: ToProtocolSchema,
{
    fn protocol_schema() -> ProtocolSchema {
        let schema = schemars::schema_for!(T);
        ProtocolSchema::Recv {
            payload_type: std::any::type_name::<T>().to_string(),
            payload_schema: serde_json::to_value(&schema).unwrap_or(serde_json::Value::Null),
            then: Box::new(S::protocol_schema()),
        }
    }
}

// Implement for Loop<S>
impl<S> ToProtocolSchema for Loop<S>
where
    S: ToProtocolSchema,
{
    fn protocol_schema() -> ProtocolSchema {
        ProtocolSchema::Loop {
            body: Box::new(S::protocol_schema()),
        }
    }
}

// For Choose and Offer, we need tuple impls
// Choose<(A, B)> where A and B are session types
impl<A, B> ToProtocolSchema for Choose<(A, B)>
where
    A: ToProtocolSchema,
    B: ToProtocolSchema,
{
    fn protocol_schema() -> ProtocolSchema {
        ProtocolSchema::Choose {
            branches: vec![A::protocol_schema(), B::protocol_schema()],
        }
    }
}

impl<A, B, C> ToProtocolSchema for Choose<(A, B, C)>
where
    A: ToProtocolSchema,
    B: ToProtocolSchema,
    C: ToProtocolSchema,
{
    fn protocol_schema() -> ProtocolSchema {
        ProtocolSchema::Choose {
            branches: vec![
                A::protocol_schema(),
                B::protocol_schema(),
                C::protocol_schema(),
            ],
        }
    }
}

impl<A, B> ToProtocolSchema for Offer<(A, B)>
where
    A: ToProtocolSchema,
    B: ToProtocolSchema,
{
    fn protocol_schema() -> ProtocolSchema {
        ProtocolSchema::Offer {
            branches: vec![A::protocol_schema(), B::protocol_schema()],
        }
    }
}

impl<A, B, C> ToProtocolSchema for Offer<(A, B, C)>
where
    A: ToProtocolSchema,
    B: ToProtocolSchema,
    C: ToProtocolSchema,
{
    fn protocol_schema() -> ProtocolSchema {
        ProtocolSchema::Offer {
            branches: vec![
                A::protocol_schema(),
                B::protocol_schema(),
                C::protocol_schema(),
            ],
        }
    }
}

// ============================================================================
// Part 15: The Recommended Approach
// ============================================================================

// Based on this exploration, the recommended approach for hub-macro is:
//
// 1. User defines stream item enum with #[derive(SessionStream)]
// 2. Derive macro generates:
//    - SessionItem impl (is_terminal)
//    - Associated Protocol type (the session type)
//    - Schema extraction
//
// 3. In #[hub_methods], user writes:
//    ```
//    #[hub_method]
//    async fn execute(&self, cmd: String) -> impl Stream<Item = BashStreamItem> {
//        self.executor.run(&cmd).await
//    }
//    ```
//
// 4. The macro sees Stream<Item = T> where T: SessionStream and:
//    - Uses T::Protocol for schema generation
//    - Uses Stream<Item = T> for RPC implementation
//    - Unified: one definition, two uses!

// ============================================================================
// Part 16: Session Type ↔ Stream Interop
// ============================================================================

/// A stream that is typed by a session protocol.
/// This is the bridge between session types and streams!
pub trait SessionStream: Stream {
    /// The session type that describes this stream's protocol
    type Protocol: ToProtocolSchema;

    /// Get the protocol schema for this stream
    fn protocol_schema() -> ProtocolSchema {
        Self::Protocol::protocol_schema()
    }
}

/// Marker trait: this stream item enum was derived from a session type
pub trait FromSessionType: SessionItem {
    /// The session type this enum was derived from
    type Protocol: ToProtocolSchema;
}

// Implement for BashStreamItem
impl FromSessionType for BashStreamItem {
    // The loop body - what gets sent in each iteration
    type Protocol = Loop<Choose<(
        SessionSend<BashStreamItem, Continue<0>>,
        SessionSend<BashStreamItem, Done>,
    )>>;
}

/// Wrap any Stream<Item = T> where T: FromSessionType to get a SessionStream
pub struct TypedStream<S, T>
where
    S: Stream<Item = T>,
    T: FromSessionType,
{
    inner: S,
    _phantom: std::marker::PhantomData<T>,
}

impl<S, T> TypedStream<S, T>
where
    S: Stream<Item = T>,
    T: FromSessionType,
{
    pub fn new(stream: S) -> Self {
        Self {
            inner: stream,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<S, T> Stream for TypedStream<S, T>
where
    S: Stream<Item = T>,
    T: FromSessionType,
{
    type Item = T;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // SAFETY: we're not moving the inner stream
        unsafe { self.map_unchecked_mut(|s| &mut s.inner) }.poll_next(cx)
    }
}

impl<S, T> SessionStream for TypedStream<S, T>
where
    S: Stream<Item = T>,
    T: FromSessionType,
{
    type Protocol = T::Protocol;
}

// ============================================================================
// Part 17: Runtime Protocol Validation
// ============================================================================

/// Validates that a stream conforms to a session type at runtime.
/// This is useful for validating streams from untrusted sources.
pub struct ValidatingStream<S, T>
where
    S: Stream<Item = T>,
    T: SessionItem,
{
    inner: S,
    terminated: bool,
    _phantom: std::marker::PhantomData<T>,
}

impl<S, T> ValidatingStream<S, T>
where
    S: Stream<Item = T>,
    T: SessionItem,
{
    pub fn new(stream: S) -> Self {
        Self {
            inner: stream,
            terminated: false,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<S, T> Stream for ValidatingStream<S, T>
where
    S: Stream<Item = T> + Unpin,
    T: SessionItem + Unpin,
{
    type Item = Result<T, ProtocolViolation>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;

        // Get mutable access to self
        let this = self.get_mut();

        if this.terminated {
            // Protocol says we're done - no more items allowed
            return match Pin::new(&mut this.inner).poll_next(cx) {
                Poll::Ready(Some(_)) => {
                    Poll::Ready(Some(Err(ProtocolViolation::ItemAfterTerminal)))
                }
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            };
        }

        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(Some(item)) => {
                if item.is_terminal() {
                    this.terminated = true;
                }
                Poll::Ready(Some(Ok(item)))
            }
            Poll::Ready(None) => {
                if !this.terminated {
                    // Stream ended without terminal item!
                    Poll::Ready(Some(Err(ProtocolViolation::NoTerminalItem)))
                } else {
                    Poll::Ready(None)
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ProtocolViolation {
    /// Stream produced an item after the terminal item
    ItemAfterTerminal,
    /// Stream ended without producing a terminal item
    NoTerminalItem,
}

// ============================================================================
// Part 18: Converting Session Types to Stream Item Enums (Macro Support)
// ============================================================================

/// This trait extracts the payload types from Choose branches.
/// A macro would use this to generate the stream item enum.
pub trait ExtractChoiceBranches {
    /// Returns info about each branch for code generation
    fn branch_info() -> Vec<BranchInfo>;
}

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub payload_type: String,
    pub is_terminal: bool,
}

// For a 2-branch Choose
impl<A, B> ExtractChoiceBranches for Choose<(A, B)>
where
    A: ExtractSendType,
    B: ExtractSendType,
    A::Continuation: IsTerminal,
    B::Continuation: IsTerminal,
{
    fn branch_info() -> Vec<BranchInfo> {
        vec![
            BranchInfo {
                payload_type: std::any::type_name::<A::Payload>().to_string(),
                is_terminal: A::Continuation::IS_TERMINAL,
            },
            BranchInfo {
                payload_type: std::any::type_name::<B::Payload>().to_string(),
                is_terminal: B::Continuation::IS_TERMINAL,
            },
        ]
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_item_terminal() {
        assert!(!BashStreamItem::Stdout(StdoutEvent { line: "x".into() }).is_terminal());
        assert!(!BashStreamItem::Stderr(StderrEvent { line: "x".into() }).is_terminal());
        assert!(BashStreamItem::Exit(ExitEvent { code: 0 }).is_terminal());
    }

    #[test]
    fn test_is_terminal_trait() {
        // Compile-time check using the IsTerminal trait
        assert!(<Done as IsTerminal>::IS_TERMINAL);
        assert!(!<Continue<0> as IsTerminal>::IS_TERMINAL);
        assert!(!<SessionSend<String, Continue<0>> as IsTerminal>::IS_TERMINAL);
        assert!(<SessionSend<String, Done> as IsTerminal>::IS_TERMINAL);
    }

    #[test]
    fn test_extract_schema_from_session_type() {
        // THE KEY TEST: Extract schema directly from a session type!

        // Define a session type
        type MyProtocol = Recv<String, Loop<Choose<(
            SessionSend<StdoutEvent, Continue<0>>,
            SessionSend<ExitEvent, Done>,
        )>>>;

        // Extract its schema
        let schema = <MyProtocol as ToProtocolSchema>::protocol_schema();

        // Verify structure
        match &schema {
            ProtocolSchema::Recv { payload_type, then, .. } => {
                assert!(payload_type.contains("String"));
                match then.as_ref() {
                    ProtocolSchema::Loop { body } => {
                        match body.as_ref() {
                            ProtocolSchema::Choose { branches } => {
                                assert_eq!(branches.len(), 2);

                                // First branch: Send<StdoutEvent, Continue<0>>
                                match &branches[0] {
                                    ProtocolSchema::Send { payload_type, then, .. } => {
                                        assert!(payload_type.contains("StdoutEvent"));
                                        assert!(matches!(then.as_ref(), ProtocolSchema::Continue { index: 0 }));
                                    }
                                    _ => panic!("Expected Send in first branch"),
                                }

                                // Second branch: Send<ExitEvent, Done>
                                match &branches[1] {
                                    ProtocolSchema::Send { payload_type, then, .. } => {
                                        assert!(payload_type.contains("ExitEvent"));
                                        assert!(matches!(then.as_ref(), ProtocolSchema::Done));
                                    }
                                    _ => panic!("Expected Send in second branch"),
                                }
                            }
                            _ => panic!("Expected Choose in loop body"),
                        }
                    }
                    _ => panic!("Expected Loop after Recv"),
                }
            }
            _ => panic!("Expected Recv at start"),
        }

        println!("Extracted schema: {}", serde_json::to_string_pretty(&schema).unwrap());
    }

    #[test]
    fn test_schema_contains_json_schema_for_types() {
        // The schema should include JSON Schema for each payload type
        type SimpleProtocol = SessionSend<StdoutEvent, Done>;

        let schema = <SimpleProtocol as ToProtocolSchema>::protocol_schema();

        match schema {
            ProtocolSchema::Send { payload_schema, .. } => {
                // Should have properties from StdoutEvent
                let schema_str = serde_json::to_string(&payload_schema).unwrap();
                assert!(schema_str.contains("line"), "Should have 'line' property");
                assert!(schema_str.contains("string"), "Should have string type");
            }
            _ => panic!("Expected Send"),
        }
    }

    #[test]
    fn test_full_bash_protocol_schema() {
        // Test with our actual BashStreamProtocol type
        let schema = <BashStreamProtocol as ToProtocolSchema>::protocol_schema();

        // Print it for visual inspection
        let json = serde_json::to_string_pretty(&schema).unwrap();
        println!("BashStreamProtocol schema:\n{}", json);

        // Verify it's well-formed (note: serde renames to snake_case)
        assert!(json.contains("recv"));
        assert!(json.contains("loop"));
        assert!(json.contains("choose"));
    }

    // ========================================================================
    // Session Type ↔ Stream Interop Tests
    // ========================================================================

    #[tokio::test]
    async fn test_typed_stream_provides_schema() {
        // Create a regular stream
        let stream = execute_typed("test");

        // Wrap it in a TypedStream to associate it with a session type
        let _typed = TypedStream::<_, BashStreamItem>::new(stream);

        // Get the schema from the stream item type directly
        // (The TypedStream just carries the association)
        let schema = <BashStreamItem as FromSessionType>::Protocol::protocol_schema();

        let json = serde_json::to_string_pretty(&schema).unwrap();
        println!("TypedStream schema:\n{}", json);

        assert!(json.contains("loop"));
        assert!(json.contains("choose"));
    }

    #[tokio::test]
    async fn test_validating_stream_catches_missing_terminal() {
        // A stream that ends without a terminal item (protocol violation!)
        let bad_stream = async_stream::stream! {
            yield BashStreamItem::Stdout(StdoutEvent { line: "hello".into() });
            // Oops! No Exit event - stream just ends
        };

        let mut validating = ValidatingStream::new(Box::pin(bad_stream));

        // First item is fine
        let item1 = validating.next().await;
        assert!(matches!(item1, Some(Ok(BashStreamItem::Stdout(_)))));

        // Stream ends - should report protocol violation
        let item2 = validating.next().await;
        assert!(matches!(item2, Some(Err(ProtocolViolation::NoTerminalItem))));
    }

    #[tokio::test]
    async fn test_validating_stream_catches_item_after_terminal() {
        // A stream that sends items after the terminal (protocol violation!)
        let bad_stream = async_stream::stream! {
            yield BashStreamItem::Exit(ExitEvent { code: 0 }); // Terminal
            yield BashStreamItem::Stdout(StdoutEvent { line: "after exit!".into() }); // Bad!
        };

        let mut validating = ValidatingStream::new(Box::pin(bad_stream));

        // First item is the terminal
        let item1 = validating.next().await;
        assert!(matches!(item1, Some(Ok(BashStreamItem::Exit(_)))));

        // Second item violates the protocol
        let item2 = validating.next().await;
        assert!(matches!(item2, Some(Err(ProtocolViolation::ItemAfterTerminal))));
    }

    #[tokio::test]
    async fn test_validating_stream_accepts_valid_protocol() {
        // A valid stream that follows the protocol
        let good_stream = async_stream::stream! {
            yield BashStreamItem::Stdout(StdoutEvent { line: "line 1".into() });
            yield BashStreamItem::Stderr(StderrEvent { line: "warning".into() });
            yield BashStreamItem::Exit(ExitEvent { code: 0 }); // Terminal - properly ends
        };

        let mut validating = ValidatingStream::new(Box::pin(good_stream));

        // All items should be Ok
        assert!(matches!(validating.next().await, Some(Ok(BashStreamItem::Stdout(_)))));
        assert!(matches!(validating.next().await, Some(Ok(BashStreamItem::Stderr(_)))));
        assert!(matches!(validating.next().await, Some(Ok(BashStreamItem::Exit(_)))));

        // Stream should end cleanly
        assert!(validating.next().await.is_none());
    }

    #[test]
    fn test_from_session_type_provides_protocol() {
        // BashStreamItem implements FromSessionType, giving us the protocol
        let schema = <BashStreamItem as FromSessionType>::Protocol::protocol_schema();

        let json = serde_json::to_string_pretty(&schema).unwrap();
        println!("BashStreamItem::Protocol schema:\n{}", json);

        assert!(json.contains("loop"));
    }

    #[test]
    fn test_extract_choice_branches() {
        // We can extract branch info from a Choose type
        type TestChoose = Choose<(
            SessionSend<StdoutEvent, Continue<0>>,
            SessionSend<ExitEvent, Done>,
        )>;

        let branches = TestChoose::branch_info();

        assert_eq!(branches.len(), 2);
        assert!(branches[0].payload_type.contains("StdoutEvent"));
        assert!(!branches[0].is_terminal); // Continue = not terminal
        assert!(branches[1].payload_type.contains("ExitEvent"));
        assert!(branches[1].is_terminal); // Done = terminal
    }
}
