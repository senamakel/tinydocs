//! Product-facing TinyDocs client tests over an in-process recording broker.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tinybus::broker::Broker;
use tinybus::module::{
    AccessFuture, FailureReport, FailureReporter, FailureSink, ModuleAccess, ModuleClient,
    ProxyFailure, ReleaseRecord,
};
use tinybus::service::Interface;
use tinybus::transport::memory::MemoryBus;
use tinybus::{Connection, Error, InterfaceName, MemberName, ObjectPath, Proxy};
use tinydocs_bus::names::{BUS_NAME, METHODS, OBJECT_PATH, methods};
use tinydocs_bus::{DocumentFormat, DocumentSpec, ExtractDocumentSpec, OutputRef, RenderPdfSpec};
use tinydocs_client::{DocumentCallError, TinyDocsClient};
use tokio::sync::Notify;

const MODULE_ID: &str = "tinydocs";
const CHUNK_BYTES: u64 = 1024 * 1024;
const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;

static RECORD: ReleaseRecord = ReleaseRecord {
    id: MODULE_ID,
    bus_name: BUS_NAME,
    object_path: OBJECT_PATH,
    version: "0.2.4",
    release_url: "https://github.com/tinyhumansai/tinydocs/releases/tag/v0.2.4",
    assets: &[],
};

#[derive(Default)]
struct NoopSink;

impl FailureSink for NoopSink {
    fn capture(&self, _report: &FailureReport<'_>) {}
}

struct FixtureAccess {
    connection: Connection,
    reporter: FailureReporter,
}

impl ModuleAccess for FixtureAccess {
    fn find(&self, id: &str) -> Option<&'static ReleaseRecord> {
        (id == MODULE_ID).then_some(&RECORD)
    }

    fn enabled(&self) -> bool {
        true
    }

    fn reporter(&self) -> &FailureReporter {
        &self.reporter
    }

    fn ensure_loaded<'a>(&'a self, _id: &'a str) -> AccessFuture<'a, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }

    fn proxy<'a>(
        &'a self,
        bus_name: &'a str,
        object_path: &'a str,
    ) -> AccessFuture<'a, Result<Proxy, ProxyFailure>> {
        Box::pin(async move {
            self.connection
                .proxy(bus_name, object_path, bus_name)
                .map_err(|_| ProxyFailure::Contract)
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Call {
    member: String,
    args: Value,
}

#[derive(Default)]
struct State {
    calls: Vec<Call>,
    input_streams: BTreeMap<String, Vec<u8>>,
    outputs: HashMap<String, Vec<u8>>,
    generated_bytes: Vec<u8>,
    next_output: usize,
    faults: HashMap<String, (String, String)>,
    malformed_replies: HashMap<String, Value>,
    declared_size: Option<u64>,
    declared_digest: Option<String>,
    read_mode: ReadMode,
    block_reads: bool,
    read_started: Arc<Notify>,
    release_seen: Arc<Notify>,
    resume_read: Arc<Notify>,
}

#[derive(Clone, Copy, Default)]
enum ReadMode {
    #[default]
    Normal,
    Empty,
    InvalidBase64,
    Overlong,
}

#[derive(Clone)]
struct RecordingService {
    connection: Connection,
    state: Arc<Mutex<State>>,
}

impl RecordingService {
    fn hold(&self, bytes: Vec<u8>) -> OutputRef {
        let mut state = self.state.lock().expect("fixture state lock");
        state.next_output += 1;
        let output_id = format!("output-{}", state.next_output);
        let sha256 = state
            .declared_digest
            .clone()
            .unwrap_or_else(|| digest(&bytes));
        let total_bytes = state.declared_size.unwrap_or(bytes.len() as u64);
        state.outputs.insert(output_id.clone(), bytes);
        OutputRef {
            output_id,
            total_bytes,
            sha256,
        }
    }

    async fn read_stream_arg(
        &self,
        args: &Value,
        index: usize,
        member: &str,
    ) -> tinybus::Result<()> {
        let stream: tinybus::stream::StreamRef = serde_json::from_value(args[index].clone())?;
        let bytes = self.connection.read_stream(&stream).await?;
        self.state
            .lock()
            .expect("fixture state lock")
            .input_streams
            .insert(member.to_string(), bytes);
        Ok(())
    }
}

#[async_trait]
impl Interface for RecordingService {
    fn name(&self) -> InterfaceName {
        InterfaceName::new(BUS_NAME).expect("valid TinyDocs interface")
    }

    fn members(&self) -> Vec<MemberName> {
        METHODS
            .iter()
            .map(|member| MemberName::new(*member).expect("valid TinyDocs member"))
            .collect()
    }

    async fn call(&self, member: &MemberName, args: Value) -> tinybus::Result<Value> {
        let member = member.as_str().to_string();
        let fault = {
            let mut state = self.state.lock().expect("fixture state lock");
            state.calls.push(Call {
                member: member.clone(),
                args: args.clone(),
            });
            state.faults.get(&member).cloned()
        };
        if let Some((name, message)) = fault {
            let stream_index = match member.as_str() {
                methods::GENERATE_PPTX if !args[1].is_null() => Some(1),
                methods::EXTRACT_TEXT | methods::INSPECT_IMAGE => Some(0),
                methods::EXTRACT_DOCUMENT | methods::CONVERT_MARKDOWN | methods::RENDER_PDF => {
                    Some(1)
                }
                _ => None,
            };
            if let Some(index) = stream_index {
                self.read_stream_arg(&args, index, &member).await?;
            }
            return Err(Error::MethodFailed { name, message });
        }
        let malformed = self
            .state
            .lock()
            .expect("fixture state lock")
            .malformed_replies
            .get(&member)
            .cloned();
        if let Some(reply) = malformed {
            if member == methods::EXTRACT_DOCUMENT {
                self.read_stream_arg(&args, 1, &member).await?;
            }
            return Ok(reply);
        }

        match member.as_str() {
            methods::GENERATE_DOCX => {
                let bytes = self
                    .state
                    .lock()
                    .expect("fixture state lock")
                    .generated_bytes
                    .clone();
                Ok(serde_json::to_value(self.hold(bytes))?)
            }
            methods::GENERATE_PPTX => {
                if !args[1].is_null() {
                    self.read_stream_arg(&args, 1, &member).await?;
                }
                Ok(serde_json::to_value(self.hold(b"pptx bytes".to_vec()))?)
            }
            methods::EXTRACT_TEXT => {
                self.read_stream_arg(&args, 0, &member).await?;
                Ok(serde_json::to_value(self.hold(b"extracted text".to_vec()))?)
            }
            methods::EXTRACT_DOCUMENT => {
                self.read_stream_arg(&args, 1, &member).await?;
                Ok(json!({
                    "format": "pdf",
                    "section_count": 1,
                    "sections": [{
                        "source": "page:1",
                        "index": 1,
                        "text": "section text",
                        "scanned_candidate": false
                    }],
                    "truncated": false
                }))
            }
            methods::CONVERT_MARKDOWN => {
                self.read_stream_arg(&args, 1, &member).await?;
                Ok(serde_json::to_value(self.hold(b"# converted\n".to_vec()))?)
            }
            methods::INSPECT_IMAGE => {
                self.read_stream_arg(&args, 0, &member).await?;
                Ok(json!({ "format": "PNG", "width_px": 2, "height_px": 3 }))
            }
            methods::RENDER_PDF => {
                self.read_stream_arg(&args, 1, &member).await?;
                let first = self.hold(b"page one".to_vec());
                let second = self.hold(b"page two".to_vec());
                Ok(json!({
                    "page_count": 3,
                    "pages": [
                        { "page": 1, "width": 2, "height": 3, "output": first },
                        { "page": 3, "width": 4, "height": 5, "output": second }
                    ]
                }))
            }
            methods::READ_OUTPUT => self.read_output(&args).await,
            methods::RELEASE_OUTPUT => {
                let output_id = args[0].as_str().expect("output id string");
                self.state
                    .lock()
                    .expect("fixture state lock")
                    .outputs
                    .remove(output_id);
                self.state
                    .lock()
                    .expect("fixture state lock")
                    .release_seen
                    .notify_one();
                Ok(Value::Null)
            }
            unknown => Err(Error::MethodFailed {
                name: "ai.tinyhumans.tinybus.Error.UnknownMethod".into(),
                message: format!("unexpected test member {unknown}"),
            }),
        }
    }
}

impl RecordingService {
    async fn read_output(&self, args: &Value) -> tinybus::Result<Value> {
        let output_id = args[0].as_str().expect("output id string").to_string();
        let offset = args[1].as_u64().expect("output offset");
        let requested = args[2].as_u64().expect("requested length");
        let (mode, block, started, resume, bytes) = {
            let state = self.state.lock().expect("fixture state lock");
            (
                state.read_mode,
                state.block_reads,
                state.read_started.clone(),
                state.resume_read.clone(),
                state.outputs.get(&output_id).cloned().unwrap_or_default(),
            )
        };
        if block {
            started.notify_one();
            resume.notified().await;
        }
        let value = match mode {
            ReadMode::Empty => String::new(),
            ReadMode::InvalidBase64 => "!".to_string(),
            ReadMode::Normal | ReadMode::Overlong => {
                let start = usize::try_from(offset).unwrap_or(usize::MAX);
                let wanted = usize::try_from(requested).unwrap_or(usize::MAX);
                let extra = usize::from(matches!(mode, ReadMode::Overlong));
                let end = start
                    .saturating_add(wanted)
                    .saturating_add(extra)
                    .min(bytes.len());
                BASE64.encode(bytes.get(start..end).unwrap_or_default())
            }
        };
        Ok(Value::String(value))
    }
}

struct Fixture {
    client: TinyDocsClient,
    state: Arc<Mutex<State>>,
    _broker: tokio::task::JoinHandle<tinybus::Result<()>>,
}

async fn test_fixture() -> Fixture {
    let bus = MemoryBus::new();
    let broker = Broker::new().spawn(bus.clone());
    let server = Connection::connect(bus.connect().await.expect("server transport"))
        .await
        .expect("server connection");
    let mut initial_state = State::default();
    initial_state.generated_bytes = b"docx bytes".to_vec();
    let state = Arc::new(Mutex::new(initial_state));
    server
        .serve_at(
            ObjectPath::new(OBJECT_PATH).expect("valid TinyDocs path"),
            RecordingService {
                connection: server.clone(),
                state: state.clone(),
            },
        )
        .await
        .expect("serve recording module");
    server
        .request_name(BUS_NAME)
        .await
        .expect("claim module name");

    let client_connection = Connection::connect(bus.connect().await.expect("client transport"))
        .await
        .expect("client connection");
    let proxy = client_connection
        .proxy(BUS_NAME, OBJECT_PATH, BUS_NAME)
        .expect("fixture proxy");
    let sdk_client = ModuleClient::new(FixtureAccess {
        connection: client_connection.clone(),
        reporter: FailureReporter::new(Arc::new(NoopSink)),
    })
    .with_fixture(proxy);

    Fixture {
        client: TinyDocsClient::new(sdk_client),
        state,
        _broker: broker,
    }
}

fn digest(bytes: &[u8]) -> String {
    let mut output = String::new();
    for byte in Sha256::digest(bytes) {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing into String");
    }
    output
}

fn document_spec() -> DocumentSpec {
    DocumentSpec {
        title: "Broker proof".into(),
        author: None,
        sections: vec![],
    }
}

#[tokio::test]
async fn typed_operations_use_contract_members_and_stream_document_bytes() {
    let fixture = test_fixture().await;
    let input = b"incoming PDF bytes";

    let generated = fixture
        .client
        .generate_docx(&document_spec())
        .await
        .expect("held DOCX should be assembled");
    assert_eq!(generated, b"docx bytes");

    let deck = tinydocs_bus::WirePresentationSpec {
        title: "Deck".into(),
        author: None,
        theme: None,
        slides: vec![],
    };
    let slide_images = b"slide image bytes";
    assert_eq!(
        fixture
            .client
            .generate_pptx(&deck, slide_images)
            .await
            .expect("PPTX output is assembled"),
        b"pptx bytes"
    );
    assert_eq!(
        fixture
            .client
            .generate_pptx(&deck, &[])
            .await
            .expect("text-only PPTX avoids an upload stream"),
        b"pptx bytes"
    );
    assert_eq!(
        fixture
            .client
            .extract_text(input)
            .await
            .expect("text is read"),
        "extracted text"
    );
    assert_eq!(
        fixture
            .client
            .convert_markdown(DocumentFormat::Pdf, input)
            .await
            .expect("Markdown is read"),
        "# converted\n"
    );

    let extracted = fixture
        .client
        .extract_document(input, &ExtractDocumentSpec::new(DocumentFormat::Pdf))
        .await
        .expect("typed extraction succeeds");
    assert_eq!(extracted.sections[0].text, "section text");

    let image = fixture
        .client
        .inspect_image(b"PNG stream")
        .await
        .expect("image facts are decoded");
    assert_eq!((image.width_px, image.height_px), (2, 3));

    let state = fixture.state.lock().expect("fixture state lock");
    assert_eq!(state.input_streams[methods::EXTRACT_DOCUMENT], input);
    assert_eq!(state.input_streams[methods::INSPECT_IMAGE], b"PNG stream");
    assert_eq!(state.input_streams[methods::GENERATE_PPTX], slide_images);
    assert_eq!(state.input_streams[methods::EXTRACT_TEXT], input);
    assert_eq!(state.input_streams[methods::CONVERT_MARKDOWN], input);
    assert!(state.calls.iter().any(|call| {
        call.member == methods::GENERATE_DOCX && call.args[0]["title"] == "Broker proof"
    }));
    assert!(
        state
            .calls
            .iter()
            .any(|call| { call.member == methods::GENERATE_PPTX && call.args[1].is_null() })
    );
}

#[tokio::test]
async fn module_wire_failures_keep_invalid_input_distinct_from_execution_failure() {
    let fixture = test_fixture().await;
    fixture
        .state
        .lock()
        .expect("fixture state lock")
        .faults
        .insert(
            methods::GENERATE_DOCX.to_string(),
            (
                "ai.tinyhumans.tinydocs.Error.InvalidInput".into(),
                "title is empty".into(),
            ),
        );
    assert!(matches!(
        fixture.client.generate_docx(&document_spec()).await,
        Err(DocumentCallError::InvalidInput(_))
    ));

    fixture
        .state
        .lock()
        .expect("fixture state lock")
        .faults
        .insert(
            methods::GENERATE_DOCX.to_string(),
            (
                "ai.tinyhumans.tinydocs.Error.GenerationFailed".into(),
                "private generator detail".into(),
            ),
        );
    assert!(matches!(
        fixture
            .client
            .generate_docx(&document_spec())
            .await
            .expect_err("generation failure is returned"),
        DocumentCallError::Failed(_)
    ));

    fixture
        .state
        .lock()
        .expect("fixture state lock")
        .faults
        .insert(
            methods::GENERATE_DOCX.to_string(),
            (
                "ai.tinyhumans.tinybus.Error.ModuleUnavailable".into(),
                "module is down".into(),
            ),
        );
    assert!(matches!(
        fixture.client.generate_docx(&document_spec()).await,
        Err(DocumentCallError::Unavailable(_))
    ));

    let fixture = test_fixture().await;
    fixture
        .state
        .lock()
        .expect("fixture state lock")
        .faults
        .insert(
            methods::EXTRACT_DOCUMENT.to_string(),
            (
                "ai.tinyhumans.tinydocs.Error.InvalidInput".into(),
                "PDF header is invalid".into(),
            ),
        );
    assert!(matches!(
        fixture
            .client
            .extract_document(
                b"invalid PDF",
                &ExtractDocumentSpec::new(DocumentFormat::Pdf)
            )
            .await,
        Err(DocumentCallError::InvalidInput(_))
    ));

    let fixture = test_fixture().await;
    fixture
        .state
        .lock()
        .expect("fixture state lock")
        .faults
        .insert(
            methods::INSPECT_IMAGE.to_string(),
            (
                "ai.tinyhumans.tinydocs.Error.ExtractionFailed".into(),
                "image parser failed".into(),
            ),
        );
    assert!(matches!(
        fixture.client.inspect_image(b"PNG bytes").await,
        Err(DocumentCallError::Failed(_))
    ));

    let fixture = test_fixture().await;
    fixture
        .state
        .lock()
        .expect("fixture state lock")
        .faults
        .insert(
            methods::READ_OUTPUT.to_string(),
            (
                "ai.tinyhumans.tinydocs.Error.InvalidInput".into(),
                "output range is invalid".into(),
            ),
        );
    assert!(matches!(
        fixture.client.generate_docx(&document_spec()).await,
        Err(DocumentCallError::InvalidInput(_))
    ));
    let state = fixture.state.lock().expect("fixture state lock");
    assert!(
        state
            .calls
            .iter()
            .any(|call| call.member == methods::RELEASE_OUTPUT)
    );
    assert!(state.outputs.is_empty());

    let fixture = test_fixture().await;
    fixture
        .state
        .lock()
        .expect("fixture state lock")
        .malformed_replies
        .insert(
            methods::EXTRACT_DOCUMENT.to_string(),
            json!({"truncated": "wrong type"}),
        );
    let error = fixture
        .client
        .extract_document(b"PDF bytes", &ExtractDocumentSpec::new(DocumentFormat::Pdf))
        .await
        .expect_err("a malformed typed reply is rejected");
    assert!(matches!(error, DocumentCallError::Failed(_)));
}

#[tokio::test]
async fn held_outputs_are_chunked_in_order_verified_and_released() {
    let fixture = test_fixture().await;
    let bytes = vec![0x5a; (CHUNK_BYTES * 2 + 13) as usize];
    {
        let mut state = fixture.state.lock().expect("fixture state lock");
        state.generated_bytes = bytes.clone();
        state.declared_digest = Some(digest(&bytes));
    }
    let got = fixture
        .client
        .generate_docx(&document_spec())
        .await
        .expect("output is assembled");
    assert_eq!(got, bytes);
    let offsets = fixture
        .state
        .lock()
        .expect("fixture state lock")
        .calls
        .iter()
        .filter(|call| call.member == methods::READ_OUTPUT)
        .map(|call| call.args[1].as_u64().expect("read offset"))
        .collect::<Vec<_>>();
    assert_eq!(offsets, [0, CHUNK_BYTES, CHUNK_BYTES * 2]);
    assert!(
        fixture
            .state
            .lock()
            .expect("fixture state lock")
            .outputs
            .is_empty()
    );

    let fixture = test_fixture().await;
    {
        let mut state = fixture.state.lock().expect("fixture state lock");
        state.generated_bytes = b"digest must reject this output".to_vec();
        state.declared_digest = Some("0".repeat(64));
    }
    assert!(matches!(
        fixture.client.generate_docx(&document_spec()).await,
        Err(DocumentCallError::Failed(_))
    ));
    let state = fixture.state.lock().expect("fixture state lock");
    assert!(
        state
            .calls
            .iter()
            .any(|call| call.member == methods::RELEASE_OUTPUT)
    );
    assert!(state.outputs.is_empty());

    let fixture = test_fixture().await;
    fixture
        .state
        .lock()
        .expect("fixture state lock")
        .declared_size = Some(MAX_DOCUMENT_BYTES + 1);
    assert!(matches!(
        fixture.client.generate_docx(&document_spec()).await,
        Err(DocumentCallError::Failed(_))
    ));
    let state = fixture.state.lock().expect("fixture state lock");
    assert!(
        !state
            .calls
            .iter()
            .any(|call| call.member == methods::READ_OUTPUT)
    );
    assert!(
        state
            .calls
            .iter()
            .any(|call| call.member == methods::RELEASE_OUTPUT)
    );

    for mode in [ReadMode::Empty, ReadMode::InvalidBase64, ReadMode::Overlong] {
        let fixture = test_fixture().await;
        {
            let mut state = fixture.state.lock().expect("fixture state lock");
            state.read_mode = mode;
            if matches!(mode, ReadMode::Overlong) {
                state.generated_bytes = vec![0x33; CHUNK_BYTES as usize + 16];
                state.declared_size = Some(CHUNK_BYTES);
                let declared_digest = digest(&state.generated_bytes);
                state.declared_digest = Some(declared_digest);
            }
        }
        assert!(matches!(
            fixture.client.generate_docx(&document_spec()).await,
            Err(DocumentCallError::Failed(_))
        ));
        assert!(
            fixture
                .state
                .lock()
                .expect("fixture state lock")
                .calls
                .iter()
                .any(|call| call.member == methods::RELEASE_OUTPUT)
        );
    }
}

#[tokio::test]
async fn cancellation_releases_held_output_and_render_releases_every_page() {
    let fixture = test_fixture().await;
    fixture
        .state
        .lock()
        .expect("fixture state lock")
        .block_reads = true;
    let started = fixture
        .state
        .lock()
        .expect("fixture state lock")
        .read_started
        .clone();
    let release_seen = fixture
        .state
        .lock()
        .expect("fixture state lock")
        .release_seen
        .clone();
    let client = fixture.client.clone();
    let call = tokio::spawn(async move { client.generate_docx(&document_spec()).await });
    started.notified().await;
    call.abort();
    let _ = call.await;
    release_seen.notified().await;
    let resume_read = fixture
        .state
        .lock()
        .expect("fixture state lock")
        .resume_read
        .clone();
    resume_read.notify_one();
    assert!(
        fixture
            .state
            .lock()
            .expect("fixture state lock")
            .outputs
            .is_empty()
    );

    let fixture = test_fixture().await;
    let rendered = fixture
        .client
        .render_pdf(
            b"PDF input",
            &RenderPdfSpec {
                pages: vec![1, 3],
                max_dimension: 256,
                max_total_pixels: 65_536,
                max_output_bytes: 1_048_576,
            },
        )
        .await
        .expect("selected pages are collected");
    assert_eq!(
        rendered.iter().map(|(page, _)| *page).collect::<Vec<_>>(),
        vec![1, 3]
    );
    assert!(
        fixture
            .state
            .lock()
            .expect("fixture state lock")
            .outputs
            .is_empty()
    );

    let fixture = test_fixture().await;
    fixture.state.lock().expect("fixture state lock").read_mode = ReadMode::InvalidBase64;
    assert!(matches!(
        fixture
            .client
            .render_pdf(
                b"PDF input",
                &RenderPdfSpec {
                    pages: vec![1, 3],
                    max_dimension: 256,
                    max_total_pixels: 65_536,
                    max_output_bytes: 1_048_576,
                },
            )
            .await,
        Err(DocumentCallError::Failed(_))
    ));
    let state = fixture.state.lock().expect("fixture state lock");
    assert!(state.outputs.is_empty());
    assert_eq!(
        state
            .calls
            .iter()
            .filter(|call| call.member == methods::RELEASE_OUTPUT)
            .count(),
        2
    );
}
