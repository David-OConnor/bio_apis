//! Molecular dynamics simulations from the [MDposit REST API](https://mdposit.mddbr.eu/api/rest/docs/).
//!
//! Use [`Client::files`] to find deposited filenames and [`Client::download_file`] to
//! retrieve them, including Amber PRMTOP files. [`Client::download_trajectory`] exports
//! a trajectory in the requested format and can select frames or atoms.
//!
//! ```no_run
//! use std::{fs::File, io};
//!
//! use bio_apis::mdposit::{Client, Params, TrajectoryFormat};
//!
//! fn main() -> io::Result<()> {
//!     let client = Client::new();
//!     let project = "MD-A00001";
//!     println!("{:?}", client.files(project)?);
//!     client.download_file(project, "topology.prmtop", &mut File::create("topology.prmtop")?)?;
//!     client.download_trajectory(
//!         project,
//!         TrajectoryFormat::Xtc,
//!         Params::Query(&[("frames", "1-10")]),
//!         &mut File::create("first-ten.xtc")?,
//!     )?;
//!     Ok(())
//! }
//! ```

use std::{
    io::{self, BufReader, Cursor, Read, Write},
    time::Duration,
};

use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use ureq::{
    Agent, Body, SendBody,
    http::{Response, StatusCode, header},
};
use url::Url;

pub const BASE_URL: &str = "https://mdposit.mddbr.eu/api/rest/v2";

/// Arguments for endpoints supporting both GET and POST.
#[derive(Clone, Copy, Debug)]
pub enum Params<'a> {
    /// GET with URL-encoded query pairs. A key may appear more than once.
    Query(&'a [(&'a str, &'a str)]),
    /// POST with an object of documented parameters, including arrays for long selections.
    Json(&'a Value),
}

impl Default for Params<'_> {
    fn default() -> Self {
        Self::Query(&[])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrajectoryFormat {
    /// MDposit's web client format.
    Bin,
    Mdcrd,
    Xtc,
    Trr,
    Nc,
    Dcd,
}

impl TrajectoryFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bin => "bin",
            Self::Mdcrd => "mdcrd",
            Self::Xtc => "xtc",
            Self::Trr => "trr",
            Self::Nc => "nc",
            Self::Dcd => "dcd",
        }
    }
}

/// Categories shared by the references and pointers endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceKind {
    Proteins,
    Pdbs,
    InchiKeys,
    Chains,
    Collections,
}

impl ReferenceKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Proteins => "proteins",
            Self::Pdbs => "pdbs",
            Self::InchiKeys => "inchikeys",
            Self::Chains => "chains",
            Self::Collections => "collections",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPage {
    /// Records can contain a subset of fields when a `return` projection is used.
    pub projects: Vec<Value>,
    pub filtered_count: u64,
    pub total_count: Option<u64>,
    pub total_mds_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDescription {
    pub filename: String,
    /// Size in bytes.
    pub length: Option<u64>,
    pub md5: Option<String>,
    pub content_type: Option<String>,
    /// File-specific fields, including dimension names for some binary files.
    pub metadata: Option<Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// Blocking client for the public MDposit API. Reuse it across requests.
///
/// Download methods stream into the supplied writer and return the number of bytes written.
/// A read or write error can leave a partial download in the writer.
pub struct Client {
    base_url: Url,
    agent: Agent,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Connect to the global MDposit index using the v2 REST API.
    pub fn new() -> Self {
        let agent = Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            // An export may take time to generate. Limit stalls without imposing a total
            // deadline on a large transfer.
            .timeout_per_read(Some(Duration::from_secs(300)))
            .timeout_per_write(Some(Duration::from_secs(60)))
            .build()
            .into();

        Self {
            base_url: Url::parse(BASE_URL).expect("the MDposit base URL is valid"),
            agent,
        }
    }

    /// Use a different REST API base, including the version, such as
    /// `https://irb.mddbr.eu/api/rest/v2` for a specific node.
    pub fn with_base_url(mut self, base_url: &str) -> io::Result<Self> {
        let url =
            Url::parse(base_url).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected an HTTP(S) REST base URL without a query or fragment",
            ));
        }
        self.base_url = url;
        Ok(self)
    }

    /// List or search projects. Arguments include `ids`, `search`, `filter`, `return`,
    /// `raw`, `sort`, `countMds`, `limit`, and `page`.
    pub fn projects(&self, params: Params<'_>) -> io::Result<ProjectPage> {
        self.json(&["projects"], params)
    }

    /// Retrieve distinct field values and their counts. Choose fields with `return`.
    pub fn project_options(&self, query: &[(&str, &str)]) -> io::Result<Value> {
        self.json(&["projects", "options"], Params::Query(query))
    }

    /// Retrieve aggregate project counts and simulation time. Accepts `filter`.
    pub fn project_summary(&self, query: &[(&str, &str)]) -> io::Result<Value> {
        self.json(&["projects", "summary"], Params::Query(query))
    }

    /// Retrieve one project or replica by accession or internal ID. Accepts `raw`.
    pub fn project(&self, project: &str, query: &[(&str, &str)]) -> io::Result<Value> {
        self.json(&["projects", project], Params::Query(query))
    }

    /// Retrieve available metadata, topology, and analysis versions for a project.
    pub fn project_versions(&self, project: &str) -> io::Result<Value> {
        self.json(&["projects", project, "versions"], Params::default())
    }

    /// Retrieve the topology as JSON. To get a deposited PRMTOP, use [`Self::download_file`].
    /// Supports `include`, `exclude`, `atomindices`, `resindices`, `chaindices`,
    /// `dupbonds`, and `raw`.
    pub fn topology(&self, project: &str, params: Params<'_>) -> io::Result<Value> {
        self.json(&["projects", project, "topology"], params)
    }

    /// Download the PDB (default) or mmCIF structure. Set `format` to `mmcif` for mmCIF.
    /// Accepts `selection` (NGL syntax), `numbering`, and zero-based `atomindices`,
    /// `resindices`, or `chaindices`.
    pub fn download_structure(
        &self,
        project: &str,
        params: Params<'_>,
        dest: &mut dyn Write,
    ) -> io::Result<u64> {
        self.download(&["projects", project, "structure"], params, &[], dest)
    }

    /// Export a complete or selected trajectory. Supply the format via `format`.
    /// Parameters include one-based `frames` (e.g. `1-10,20`), NGL `selection`,
    /// `numbering`, and zero-based `atomindices`, `resindices`, or `chaindices`.
    pub fn download_trajectory(
        &self,
        project: &str,
        format: TrajectoryFormat,
        params: Params<'_>,
        dest: &mut dyn Write,
    ) -> io::Result<u64> {
        self.download(
            &["projects", project, "trajectory"],
            params,
            &[("format", format.as_str())],
            dest,
        )
    }

    /// Download coordinates in MDposit's binary format, with the same selection
    /// parameters as [`Self::download_trajectory`].
    pub fn download_coordinates(
        &self,
        project: &str,
        params: Params<'_>,
        dest: &mut dyn Write,
    ) -> io::Result<u64> {
        self.download(
            &["projects", project, "trajectory", "coordinates"],
            params,
            &[],
            dest,
        )
    }

    /// Download the simulation box if available. Each frame has three vectors of
    /// three little-endian `f32` values (36 bytes). Accepts one-based `frames`.
    pub fn download_box(
        &self,
        project: &str,
        params: Params<'_>,
        dest: &mut dyn Write,
    ) -> io::Result<u64> {
        self.download(
            &["projects", project, "trajectory", "box"],
            params,
            &[],
            dest,
        )
    }

    /// List filenames available for a project or replica.
    pub fn files(&self, project: &str) -> io::Result<Vec<String>> {
        self.json(&["projects", project, "files"], Params::default())
    }

    /// Stream a deposited file by its exact name from [`Self::files`].
    pub fn download_file(
        &self,
        project: &str,
        filename: &str,
        dest: &mut dyn Write,
    ) -> io::Result<u64> {
        self.download_file_with_query(project, filename, &[], dest)
    }

    /// Download a file with query parameters. For `.bin` files, pass dimension names
    /// or letters from [`Self::file_note`] with ranges, or use `parse=true`.
    pub fn download_file_with_query(
        &self,
        project: &str,
        filename: &str,
        query: &[(&str, &str)],
        dest: &mut dyn Write,
    ) -> io::Result<u64> {
        self.download(
            &["projects", project, "files", filename],
            Params::Query(query),
            &[],
            dest,
        )
    }

    /// List file descriptors for a project or replica.
    pub fn file_notes(&self, project: &str) -> io::Result<Vec<FileDescription>> {
        self.json(&["projects", project, "filenotes"], Params::default())
    }

    /// Retrieve one deposited file's descriptor.
    pub fn file_note(&self, project: &str, filename: &str) -> io::Result<FileDescription> {
        self.json(
            &["projects", project, "filenotes", filename],
            Params::default(),
        )
    }

    /// List available analysis names.
    pub fn analyses(&self, project: &str) -> io::Result<Vec<String>> {
        self.json(&["projects", project, "analyses"], Params::default())
    }

    /// Retrieve an analysis by name. The response schema depends on the analysis.
    pub fn analysis(&self, project: &str, name: &str) -> io::Result<Value> {
        self.json(&["projects", project, "analyses", name], Params::default())
    }

    /// Retrieve the references associated with a project.
    pub fn project_references(&self, project: &str) -> io::Result<Vec<Value>> {
        self.json(&["projects", project, "references"], Params::default())
    }

    /// List references of one kind. Supports `ids`, `filter`, `return`, `limit`, and
    /// `page`. With `justids=true`, the API returns an array of IDs.
    pub fn references(&self, kind: ReferenceKind, params: Params<'_>) -> io::Result<Value> {
        self.json(&["references", kind.as_str()], params)
    }

    /// Retrieve one reference by its ID or chain sequence.
    pub fn reference(&self, kind: ReferenceKind, id: &str) -> io::Result<Value> {
        self.json(&["references", kind.as_str(), id], Params::default())
    }

    /// Retrieve reference versions by ID. Accepts `limit` and `page`.
    pub fn reference_versions(
        &self,
        kind: ReferenceKind,
        query: &[(&str, &str)],
    ) -> io::Result<Value> {
        self.json(
            &["references", kind.as_str(), "versions"],
            Params::Query(query),
        )
    }

    /// Retrieve JSON pointers from references of one kind to their projects.
    /// Pass `return` to include additional project fields.
    pub fn pointers(&self, kind: ReferenceKind, query: &[(&str, &str)]) -> io::Result<Value> {
        self.json(&["pointers", kind.as_str()], Params::Query(query))
    }

    /// Retrieve JSON project pointers for a specific reference. Accepts `return`.
    pub fn pointer(
        &self,
        kind: ReferenceKind,
        id: &str,
        query: &[(&str, &str)],
    ) -> io::Result<Value> {
        self.json(&["pointers", kind.as_str(), id], Params::Query(query))
    }

    /// Download project pointers as CSV. Use `None` to include every reference of
    /// this kind. Pass `return` to include additional project fields.
    pub fn download_pointers_csv(
        &self,
        kind: ReferenceKind,
        id: Option<&str>,
        query: &[(&str, &str)],
        dest: &mut dyn Write,
    ) -> io::Result<u64> {
        let path = ["pointers", kind.as_str(), id.unwrap_or_default()];
        let path = if id.is_some() { &path[..] } else { &path[..2] };
        self.download(path, Params::Query(query), &[("format", "csv")], dest)
    }

    /// List MDposit's participating nodes.
    pub fn nodes(&self) -> io::Result<Vec<Value>> {
        self.json(&["nodes"], Params::default())
    }

    /// List PDB IDs with PDBe Knowledge Base annotations.
    pub fn knowledge_pdbs(&self) -> io::Result<Vec<String>> {
        self.json(&["knowledge"], Params::default())
    }

    /// List annotated projects for a PDB ID.
    pub fn knowledge_projects(&self, pdb_id: &str) -> io::Result<Vec<String>> {
        self.json(&["knowledge", pdb_id], Params::default())
    }

    /// List annotations available for a PDB ID and project.
    pub fn knowledge_analyses(&self, pdb_id: &str, project: &str) -> io::Result<Vec<String>> {
        self.json(&["knowledge", pdb_id, project], Params::default())
    }

    /// Retrieve an annotation, such as `sasa` or `lipid-inter`, by name.
    pub fn knowledge_analysis(&self, pdb_id: &str, project: &str, name: &str) -> io::Result<Value> {
        self.json(&["knowledge", pdb_id, project, name], Params::default())
    }

    /// Send a JSON-RPC message to the unversioned MCP endpoint next to the REST version.
    /// Notifications accepted without a response body return `None`.
    pub fn mcp(&self, message: &Value) -> io::Result<Option<Value>> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .expect("the HTTP URL supports path segments")
            .pop_if_empty()
            .pop()
            .push("mcp");

        let response = self.post(url, message, "application/json, text/event-stream")?;
        if matches!(
            response.status(),
            StatusCode::ACCEPTED | StatusCode::NO_CONTENT
        ) {
            return Ok(None);
        }
        read_json(response).map(Some)
    }

    fn json<T: DeserializeOwned>(&self, path: &[&str], params: Params<'_>) -> io::Result<T> {
        read_json(self.request(path, params, &[])?)
    }

    fn download(
        &self,
        path: &[&str],
        params: Params<'_>,
        query: &[(&str, &str)],
        dest: &mut dyn Write,
    ) -> io::Result<u64> {
        let mut response = self.request(path, params, query)?;
        io::copy(&mut response.body_mut().as_reader(), dest)
    }

    fn request(
        &self,
        path: &[&str],
        params: Params<'_>,
        extra_query: &[(&str, &str)],
    ) -> io::Result<Response<Body>> {
        if path.iter().any(|part| matches!(*part, "" | "." | "..")) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid MDposit path segment",
            ));
        }
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .expect("the HTTP URL supports path segments")
            .pop_if_empty()
            .extend(path);

        let query = match params {
            Params::Query(query) => query,
            Params::Json(_) => &[],
        };
        if !query.is_empty() || !extra_query.is_empty() {
            url.query_pairs_mut()
                .extend_pairs(query.iter().chain(extra_query));
        }

        match params {
            Params::Query(_) => {
                let response = self
                    .agent
                    .get(url.as_str())
                    .call()
                    .map_err(ureq::Error::into_io)?;
                check_status(response)
            }
            Params::Json(body) => self.post(url, body, "*/*"),
        }
    }

    fn post(&self, mut url: Url, body: &Value, accept: &str) -> io::Result<Response<Body>> {
        let bytes = serde_json::to_vec(body)?;
        // The index sends 307 redirects to project nodes. Recreate the body reader
        // for each request so a redirected node receives the complete JSON payload.
        for _ in 0..=5 {
            let mut reader = Cursor::new(&bytes);
            let response = self
                .agent
                .post(url.as_str())
                .config()
                .max_redirects(0)
                .build()
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::CONTENT_LENGTH, bytes.len().to_string())
                .header(header::ACCEPT, accept)
                .send(SendBody::from_reader(&mut reader))
                .map_err(ureq::Error::into_io)?;
            if !matches!(
                response.status(),
                StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT
            ) {
                return check_status(response);
            }
            let location = response
                .headers()
                .get(header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "MDposit redirect is missing a valid Location header",
                    )
                })?;
            url = url
                .join(location)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        }
        Err(io::Error::other("too many MDposit redirects"))
    }
}

fn read_json<T: DeserializeOwned>(mut response: Response<Body>) -> io::Result<T> {
    serde_json::from_reader(BufReader::new(response.body_mut().as_reader()))
        .map_err(io::Error::from)
}

#[cfg(test)]
mod tests {
    use std::{
        io::BufRead,
        net::{TcpListener, TcpStream},
        thread,
    };

    use super::*;

    fn read_request(stream: &mut TcpStream) -> (String, Value) {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(stream);
        let mut request_line = String::new();
        reader.read_line(&mut request_line).unwrap();

        let mut length = 0;
        for line in reader.by_ref().lines() {
            let line = line.unwrap();
            if line.is_empty() {
                break;
            }
            let (name, value) = line.split_once(':').unwrap();
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        (request_line, serde_json::from_slice(&body).unwrap())
    }

    #[test]
    fn long_trajectory_selection_reaches_redirected_node() {
        let index = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let node = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let index_url = format!("http://{}/api/rest/v2/", index.local_addr().unwrap());
        let node_url = format!("http://{}", node.local_addr().unwrap());
        let params =
            serde_json::json!({"frames": "1", "atomindices": (0..10_000).collect::<Vec<_>>()});
        let expected = params.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = index.accept().unwrap();
            let (request, body) = read_request(&mut stream);
            assert_eq!(
                request,
                "POST /api/rest/v2/projects/MD-A00001/trajectory?format=xtc HTTP/1.1\r\n"
            );
            assert_eq!(body, expected);
            write!(
                stream,
                "HTTP/1.1 307 Temporary Redirect\r\nLocation: {node_url}/api/rest/v2/projects/A0001/trajectory?format=xtc\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            let (mut stream, _) = node.accept().unwrap();
            let (request, body) = read_request(&mut stream);
            assert_eq!(
                request,
                "POST /api/rest/v2/projects/A0001/trajectory?format=xtc HTTP/1.1\r\n"
            );
            assert_eq!(body, expected);
            stream
                .write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nConnection: close\r\n\r\n\x00\x00\x07\xcb")
                .unwrap();
        });

        let mut client = Client::new().with_base_url(&index_url).unwrap();
        client.agent = Agent::config_builder()
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(5)))
            .build()
            .into();
        let mut data = Vec::new();
        let written = client
            .download_trajectory(
                "MD-A00001",
                TrajectoryFormat::Xtc,
                Params::Json(&params),
                &mut data,
            )
            .unwrap();
        assert_eq!(written, 4);
        assert_eq!(data, [0, 0, 7, 203]);
        server.join().unwrap();
    }
}

fn check_status(mut response: Response<Body>) -> io::Result<Response<Body>> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let kind = if status == StatusCode::NOT_FOUND {
        io::ErrorKind::NotFound
    } else {
        io::ErrorKind::Other
    };
    let mut detail = Vec::new();
    let _ = response
        .body_mut()
        .as_reader()
        .take(1024)
        .read_to_end(&mut detail);
    let detail = String::from_utf8_lossy(&detail);
    Err(io::Error::new(
        kind,
        format!("MDposit HTTP {status}: {}", detail.trim()),
    ))
}
