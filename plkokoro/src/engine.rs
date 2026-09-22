//! Warstwa inferencji (jedna sesja Kokoro w ONNX Runtime, ładowana dynamicznie).
//!
//! Różnica względem wersji Go: środowisko ORT w crate `ort` jest globalne i nie da się go zniszczyć, więc
//! `Model::unload` zwalnia sesję, ale biblioteka `libonnxruntime.so` pozostaje zmapowana do końca procesu, a przy
//! kolejnych modelach musi to być ta sama biblioteka.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ort::ep::ExecutionProviderDispatch;
use ort::session::Session;
use ort::value::Tensor;

use crate::error::{Error, Result};

pub(crate) trait Engine: Send + Sync {
    fn run(&self, ids: &[i64], style: &[f32], speed: f32) -> Result<Vec<f32>>;
}

const WANT_INPUTS: [&str; 3] = ["input_ids", "style", "speed"];
const WANT_OUTPUTS: [&str; 1] = ["waveform"];

fn ort_err(e: impl std::fmt::Display) -> Error {
    Error::Ort(e.to_string())
}

/// Ścieżka biblioteki użytej do inicjalizacji (globalna, jedna na proces).
static ORT_LIB: Mutex<Option<PathBuf>> = Mutex::new(None);

fn default_lib_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else if cfg!(windows) {
        "onnxruntime.dll"
    } else {
        "libonnxruntime.so"
    }
}

fn ensure_ort(lib: Option<&Path>) -> Result<()> {
    let path: PathBuf = lib
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("ORT_LIBRARY_PATH").filter(|v| !v.is_empty()).map(PathBuf::from))
        .or_else(|| std::env::var_os("ORT_DYLIB_PATH").filter(|v| !v.is_empty()).map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(default_lib_name())); // dlopen szuka w LD_LIBRARY_PATH / ścieżkach systemowych
    let mut guard = ORT_LIB.lock().unwrap_or_else(|e| e.into_inner());
    match guard.as_ref() {
        Some(p) if *p == path => Ok(()),
        Some(p) => Err(Error::Config(format!(
            "środowisko ONNX Runtime jest już zainicjowane z {:?} (żądano {:?}); ORT jest jedno na proces",
            p.display(),
            path.display()
        ))),
        None => {
            ort::init_from(&path)
                .map_err(|e| {
                    Error::Config(format!(
                        "inicjalizacja ONNX Runtime (biblioteka {:?}): {e}\nPodaj ścieżkę do libonnxruntime.so przez Config::ort_library albo ORT_LIBRARY_PATH (wymagana wersja >= 1.21)",
                        path.display()
                    ))
                })?
                .commit();
            // Crate `ort` zwalnia globalne środowisko przy wyjściu z procesu (ReleaseEnv w `.fini_array`); ORT 1.21/1.22
            // kończy się przy tym SIGSEGV-em (kod 139) już po poprawnym zakończeniu pracy. Trzymamy jedno dodatkowe,
            // nigdy nie zwalniane odwołanie: środowisko przeżywa proces, a system operacyjny sprząta.
            if let Ok(env) = ort::environment::Environment::current() {
                std::mem::forget(env);
            }
            *guard = Some(path);
            Ok(())
        }
    }
}

/// Nazwy providerów jak w ONNX Runtime (z sufiksem `ExecutionProvider` lub bez, wielkość liter bez znaczenia).
/// CPU jest zawsze dostępny; pozostałe wymagają odpowiedniej cechy kompilacji i biblioteki ORT z tym providerem.
fn providers(names: &[String]) -> Result<Vec<ExecutionProviderDispatch>> {
    #[allow(unused_mut)] // bez cech providerów nic nie jest dopisywane
    let mut out = Vec::new();
    for name in names {
        let key = name.to_lowercase();
        let key = key.strip_suffix("executionprovider").unwrap_or(&key);
        match key {
            "" | "cpu" => {}
            #[cfg(feature = "cuda")]
            "cuda" => out.push(ort::ep::CUDA::default().build().error_on_failure()),
            #[cfg(feature = "tensorrt")]
            "tensorrt" => out.push(ort::ep::TensorRT::default().build().error_on_failure()),
            #[cfg(feature = "rocm")]
            "rocm" => out.push(ort::ep::ROCm::default().build().error_on_failure()),
            #[cfg(feature = "migraphx")]
            "migraphx" => out.push(ort::ep::MIGraphX::default().build().error_on_failure()),
            #[cfg(feature = "coreml")]
            "coreml" => out.push(ort::ep::CoreML::default().build().error_on_failure()),
            #[cfg(feature = "directml")]
            "directml" | "dml" => out.push(ort::ep::DirectML::default().build().error_on_failure()),
            #[cfg(feature = "openvino")]
            "openvino" => out.push(ort::ep::OpenVINO::default().build().error_on_failure()),
            _ => {
                return Err(Error::Config(format!(
                    "provider {name:?} nie jest dostępny: nieznany albo niewłączony w tej kompilacji \
                     (cechy Cargo: cuda, tensorrt, rocm, migraphx, coreml, directml, openvino)"
                )))
            }
        }
    }
    Ok(out)
}

pub(crate) struct OrtEngine {
    session: Mutex<Session>, // serializuje run; sesja jest jedna
}

impl OrtEngine {
    pub(crate) fn new(lib: Option<&Path>, model_path: &Path, provider_names: &[String]) -> Result<Self> {
        let eps = providers(provider_names)?; // tanie błędy konfiguracji przed inicjalizacją biblioteki
        ensure_ort(lib)?;
        let mut builder = Session::builder().map_err(ort_err)?;
        if !eps.is_empty() {
            builder = builder.with_execution_providers(eps).map_err(ort_err)?;
        }
        let session = builder
            .commit_from_file(model_path)
            .map_err(|e| Error::Ort(format!("tworzenie sesji ONNX z {}: {e}", model_path.display())))?;
        check_io(&session)?;
        Ok(Self { session: Mutex::new(session) })
    }
}

/// Czytelny błąd, gdy model ma inne wejścia/wyjścia niż oczekiwane.
fn check_io(session: &Session) -> Result<()> {
    let have_in: BTreeSet<&str> = session.inputs().iter().map(|o| o.name()).collect();
    let have_out: BTreeSet<&str> = session.outputs().iter().map(|o| o.name()).collect();
    let miss_in: Vec<&str> = WANT_INPUTS.iter().copied().filter(|n| !have_in.contains(n)).collect();
    let miss_out: Vec<&str> = WANT_OUTPUTS.iter().copied().filter(|n| !have_out.contains(n)).collect();
    if miss_in.is_empty() && miss_out.is_empty() {
        return Ok(());
    }
    Err(Error::Ort(format!(
        "model Kokoro ma inne I/O niż oczekiwane. Brakuje wejść: {miss_in:?}, wyjść: {miss_out:?}. Jest: in={have_in:?}, out={have_out:?}"
    )))
}

impl Engine for OrtEngine {
    fn run(&self, ids: &[i64], style: &[f32], speed: f32) -> Result<Vec<f32>> {
        let ids_t = Tensor::from_array(([1usize, ids.len()], ids.to_vec())).map_err(ort_err)?;
        let style_t = Tensor::from_array(([1usize, style.len()], style.to_vec())).map_err(ort_err)?;
        let speed_t = Tensor::from_array(([1usize], vec![speed])).map_err(ort_err)?;
        let mut session = self.session.lock().unwrap_or_else(|e| e.into_inner());
        let outputs = session
            .run(ort::inputs!["input_ids" => ids_t, "style" => style_t, "speed" => speed_t])
            .map_err(|e| Error::Ort(format!("inferencja Kokoro: {e}")))?;
        let (_shape, data) = outputs["waveform"]
            .try_extract_tensor::<f32>()
            .map_err(|e| Error::Ort(format!("wyjście 'waveform' nie jest tensorem float32: {e}")))?;
        Ok(data.to_vec()) // kopia: wyjście należy do sesji
    }
}
