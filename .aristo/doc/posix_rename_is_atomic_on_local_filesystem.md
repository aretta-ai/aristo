**Aristo assumption — `posix_rename_is_atomic_on_local_filesystem`**

POSIX rename(2) is atomic on a single local filesystem (APFS, ext4, NTFS via Windows). On networked filesystems (NFS) atomicity is not guaranteed by POSIX, and our race-safety claim does not hold there. Aristo runs on a developer's local workspace; NFS-mounted .aristo/ is out of scope.

<sub>Background fact (no verification target).</sub>

---
