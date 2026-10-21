//! Exact-block SET/readback/rollback shared by the validated macOS controls.
use crate::Error;

pub(crate) trait ControlBlock<const N: usize> {
    fn get(&mut self) -> Result<[u8; N], Error>;
    fn set(&mut self, block: &[u8; N]) -> Result<(), Error>;
}

struct Rollback<'a, T: ControlBlock<N>, const N: usize> {
    io: &'a mut T,
    original: [u8; N],
    description: String,
    armed: bool,
}
impl<T: ControlBlock<N>, const N: usize> Rollback<'_, T, N> {
    fn restore(&mut self) -> Result<(), Error> {
        let mut detail = String::new();
        for _ in 0..2 {
            let set = self.io.set(&self.original);
            match self.io.get() {
                Ok(block) if block == self.original => {
                    self.armed = false;
                    return Ok(());
                }
                result => detail = format!("SET {set:?}, readback {result:?}"),
            }
        }
        Err(Error::Custom(format!(
            "RESTORE NOT VERIFIED for {}; {detail}",
            self.description
        )))
    }
}
impl<T: ControlBlock<N>, const N: usize> Drop for Rollback<'_, T, N> {
    fn drop(&mut self) {
        // Unwind fallback only. Abort, SIGKILL and power loss cannot run Drop.
        if self.armed
            && let Err(error) = self.restore()
        {
            eprintln!("macOS RM: {error}");
        }
    }
}

/// Caller validates identity, mode, units, bounds and the original snapshot,
/// and holds the transport lock throughout this transaction.
pub(crate) fn write_block<T: ControlBlock<N>, const N: usize>(
    io: &mut T,
    original: [u8; N],
    wanted: [u8; N],
    cancelled: &dyn Fn() -> bool,
    description: String,
) -> Result<(), Error> {
    if cancelled() {
        return Err(Error::Custom(format!(
            "{description}: cancelled before SET"
        )));
    }
    if original == wanted {
        return Ok(());
    }
    let mut rollback = Rollback {
        io,
        original,
        description,
        armed: true,
    };
    let outcome = (|| {
        rollback.io.set(&wanted)?;
        if rollback.io.get()? != wanted {
            return Err(Error::Custom(format!(
                "{}: full-block readback mismatch",
                rollback.description
            )));
        }
        if cancelled() {
            return Err(Error::Custom(format!(
                "{}: cancelled after SET",
                rollback.description
            )));
        }
        Ok(())
    })();
    match outcome {
        Ok(()) => {
            rollback.armed = false;
            Ok(())
        }
        Err(error) => {
            let restored = rollback.restore();
            rollback.armed = false; // No hidden third attempt in Drop.
            match restored {
                Ok(()) => Err(Error::Custom(format!(
                    "{error}; original block restored and verified"
                ))),
                Err(restore) => Err(Error::Custom(format!("{error}; {restore}"))),
            }
        }
    }
}
