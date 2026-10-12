/*
 * probe-macos-rm — RM alloc/control/free wrappers plus the client→device→
 * subdevice chain, byte-for-byte the dance NullMoth's own userspace does
 * (kexts/NVRM/kms.inc:93-100) and the NVK shim does (nvRmApi.c in
 * nvk/nvk-macos.patch).
 */
#ifndef NVX_RM_H
#define NVX_RM_H

#include "nvtypes.h"

/* RM escape on the ctl connection; returns NV_STATUS (0 = NV_OK). */
NvU32 nvx_rm_control(int ctl_fd, NvHandle hClient, NvHandle hObject,
                     NvU32 cmd, void *params, NvU32 paramsSize);

/* Returns the new object handle (nonzero) or 0 on failure (status in *st). */
NvHandle nvx_rm_alloc(int ctl_fd, NvHandle hRoot, NvHandle hParent,
                      NvHandle hObjectNew, NvU32 hClass,
                      void *allocParms, NvU32 paramsSize, NvU32 *st);

NvU32 nvx_rm_free(int ctl_fd, NvHandle hRoot, NvHandle hParent, NvHandle hObjectOld);

/*
 * Full bring-up of a private RM client:
 *   NV01_ROOT_CLIENT -> GPU_ATTACH_IDS -> NV01_DEVICE_0 -> NV20_SUBDEVICE_0.
 * Returns 0 on success. Out params receive the three handles.
 */
int nvx_rm_chain_open(int ctl_fd, NvU32 gpuId,
                      NvHandle *hClient, NvHandle *hDevice, NvHandle *hSubdevice);
void nvx_rm_chain_close(int ctl_fd, NvHandle hClient, NvHandle hDevice, NvHandle hSubdevice);

/* NV_STATUS -> printable name via the OGKM nvstatuscodes.h X-macro list. */
const char *nvx_status_name(NvU32 status);

#endif /* NVX_RM_H */
