//! Ata la vida de llama-server a la de la app: si la app se cierra, se cae o
//! la matan desde el Administrador de tareas, el sistema operativo mata también
//! el servidor. Así nunca queda un modelo ocupando la GPU en segundo plano.

/// Windows: un "Job Object" con KILL_ON_JOB_CLOSE. El handle no se cierra
/// nunca a propósito; cuando el proceso de la app termina, Windows lo cierra
/// y mata todo lo que esté dentro del job.
#[cfg(windows)]
pub fn bind_to_app(child: &tokio::process::Child) {
    use std::sync::OnceLock;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    static JOB: OnceLock<usize> = OnceLock::new();
    let job = *JOB.get_or_init(|| unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return 0;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        job as usize
    });
    if let (true, Some(handle)) = (job != 0, child.raw_handle()) {
        unsafe {
            AssignProcessToJobObject(job as _, handle as _);
        }
    }
}

#[cfg(not(windows))]
pub fn bind_to_app(_child: &tokio::process::Child) {}

/// Linux: el kernel manda SIGKILL al servidor si la app muere.
#[cfg(target_os = "linux")]
pub fn die_with_parent(cmd: &mut tokio::process::Command) {
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        });
    }
}

#[cfg(not(target_os = "linux"))]
pub fn die_with_parent(_cmd: &mut tokio::process::Command) {}

#[cfg(test)]
mod tests {
    /// Prueba manual: imprime el PID de un hijo atado a este proceso y espera.
    /// Al matar el proceso de prueba, el hijo debe morir también.
    #[tokio::test]
    #[ignore]
    async fn child_dies_with_app() {
        let mut cmd = if cfg!(windows) {
            let mut c = tokio::process::Command::new("ping");
            c.args(["-n", "120", "127.0.0.1"]);
            c
        } else {
            let mut c = tokio::process::Command::new("sleep");
            c.arg("120");
            c
        };
        cmd.stdout(std::process::Stdio::null());
        super::die_with_parent(&mut cmd);
        let child = cmd.spawn().unwrap();
        super::bind_to_app(&child);
        println!("CHILD_PID={}", child.id().unwrap());
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;
    }
}
