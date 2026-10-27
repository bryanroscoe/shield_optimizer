# Windows native ADB identity acceptance

This stacked experiment preserves the separately reviewed Mac first-run candidate.
No production credential creation, permission change, default switch or publication
is authorized by fixture results.

## Security contract

Rust mode uses its own standard per-user app-data namespace. It must not import,
replace or repair an existing ADB identity, use the working directory as storage,
or fall back to an insecure directory or a different transport.

New directories and staging files need a protected current-user-only DACL at
creation, not a later ACL repair. Existing private identity entries must have the
expected owner and private DACL. Missing, NULL, inherited or broader access on
these private entries must fail visibly. Normal parent app-data directories use
a separate trusted-mutator policy so ordinary inherited user/SYSTEM/administrator
permissions need no repair; untrusted mutation or replacement rights are refused.
Ancestor ownership must also be trusted, because an owner can change a DACL.
The narrowly identified Windows TrustedInstaller principal is permitted for OS
ancestry, not as a reader of the private key. Its SID is documented by Microsoft;
no domain/account lookup is needed.
Current-user identity is read from the process token; no token privilege changes
are part of this implementation.

Directory traversal must reject reparse points and ambiguous/device/network paths
and retain handles that prevent replacement of the selected directory chain
during initialization. Ancestor policy distinguishes replacement/control rights
from creation of unrelated siblings; ordinary Windows path compatibility still
requires runtime acceptance.
Existing key files must be regular disk files, bounded and valid RSA-2048 PKCS#8.
Private staging bytes must be complete, validated and flushed before a no-replace
publication. A concurrent complete winner must be validated and reused; failure
must not trigger overwrite, rotation or cross-driver replay. Crash leftovers and
uncertain persistence must be reported honestly.

## Verification boundary

The connected Mac has no installed Windows Rust target. No target, SDK, emulator
or runtime is installed by this work. Pure policy/fixture checks on macOS cannot
establish Win32 API behavior, Windows compilation, ACL enforcement, atomic rename,
crash durability, packaged-app behavior or TV authorization.

Windows acceptance still needs compilation and fixture execution on Windows,
including ordinary and elevated users, private/broad/inherited/NULL ACLs, corrupt
and disappearing keys, reparse/directory aliases, competing processes, interrupted
writes, publication failures, restrictive app storage and crash leftovers. Actual
production key creation and TV authorization remain separately approved actions.

## API sources

- [CreateFileW and creation security attributes](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
- [GetSecurityInfo on an opened handle](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-getsecurityinfo)
- [GetTokenInformation](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-gettokeninformation)
- [Microsoft-documented TrustedInstaller SID](https://learn.microsoft.com/en-us/archive/msdn-magazine/2008/november/access-control-understanding-windows-file-and-registry-permissions)
- [Protected DACL control](https://learn.microsoft.com/en-us/windows/win32/secauthz/security-descriptor-control)
- [FILE_RENAME_INFO and no-replace behavior](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info)

These sources describe API contracts, not evidence that this candidate passes
Windows or physical-device acceptance.
