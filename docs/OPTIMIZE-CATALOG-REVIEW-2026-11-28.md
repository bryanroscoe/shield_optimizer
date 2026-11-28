# Optimize catalog review — 2026-11-28

Assessed two previously unreviewed exact IDs from pinned CleanYourTV commit
`8672e68e8a24c1258e4662cd11daa4ed13795dad`: accepted 0, held 2.
Frequency prioritizes investigation; it is not removal-safety evidence.

## TCL voice service: hold

`com.tcl.assistant`: 26 active, 1 commented, 8 note profiles.
[TCL voice guidance](https://support.tcl.com/set-up-and-use-the-google-assistant-on-your-tcl-google-tv)
describes remote/hands-free control and model/software variability. Its
[Alexa guidance](https://support.tcl.com/en_US/us-googletv-setup-configuration/how-to-control-your-tcl-google-tv-with-alexa)
uses TCL Assistant as a skill name, which does not identify this Android package.
Neither gives its components, callers, permissions, independent disable effects
or a recovery contract. We cannot equate a display name with this exact ID.

Pinned [43P725:54–58](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-43p725/debloat.txt#L54-L58)
keeps the voice group commented for voice-remote use. On
[BeyondTV2 Android 11:79–83](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-beyondtv2-android-11/debloat.txt#L79-L83),
group disabling makes the mic button stop working; this does not isolate one
package's effect. [50EP680:28–30](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-50ep680/debloat.txt#L28-L30)
also records a grouped voice loss. In contrast,
[55S546 US:19](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-55s546-us/debloat.txt#L19)
reports Google voice working with this ID disabled on GTV93, while
[BeyondTV4 UK:27–29](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-beyondtv4-uk/debloat.txt#L27-L29)
keeps it with voice services. These firmware-specific observations cannot
establish broadly applicable optional functionality, dependencies or recovery.

## Sony partner installation configuration: hold

`android.autoinstalls.config.sony.bravia`: 25 active, 0 commented, 2 note profiles.
[Sony app installation guidance](https://helpguide.sony.net/tv/iusltn1/v1/en-003/01-02_01.html)
describes installation and deletion through the TV, not this exact package's
configuration payload, receivers, provisioning callers or recovery behavior.
Generic AOSP auto-install mechanisms cannot establish Sony's implementation.

Pinned [GB:42](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-bravia-4k-gb/debloat.txt#L42)
labels partner auto-install; [VH2:195](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-bravia-4k-vh2/debloat.txt#L195)
labels partner apps after setup. Critically,
[VH2:125–128](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-bravia-4k-vh2/debloat.txt#L125-L128)
says it actually stayed enabled: the permission block was unreadable, so the
input-driver check skipped it and no override was applied. This proves neither
an input dependency nor its absence. The profile spans Android 12 VH2 regional
units; it cannot certify disabling on all Sony TVs. Exact setup dependencies,
missed provisioning effects, re-provisioning and recovery remain unresolved.

## Independent review and validation

Implementation research and exactly one inherited-model read-only reviewer
independently checked both exact identities, primary vendor sources, pinned
keep/comment/disable observations, dependencies, disable/removal consequences,
recovery limits and enforceable applicability. Both agree HOLD until 2026-12-28
or material new evidence. The reviewer's separate primary sources were
[TCL voice setup](https://support.tcl.com/en_US/us-googletv-setup-configuration/set-up-and-use-the-google-assistant-on-your-tcl-google-tv),
[Sony initial setup](https://www.sony.com/electronics/support/televisions-projectors-lcd-tvs-android-/articles/00122502)
and [Sony app guidance](https://www.sony.co.uk/electronics/support/televisions-projectors-lcd-tvs/articles/00226737).
They do not close the exact-package evidence gaps.

No actionable entries, app lists, safety/planner logic, demo/fixtures or frontend
behavior changed. Galleries and screenshot/video evidence are inapplicable;
existing behavior evidence is unaffected. Accepted entries, imported data,
source pins, MIT notices and proprietary license remain unchanged. No native
build, packaging, hardware control, phone, billing, deployment or release.

Queue tests (4), source tests (15), provenance check (427 profiles), prior-ledger
preservation and diff audit passed. The queue now has 1,587 active candidates,
61 held identities and 88 curated entries. Hosted substantive CI is required
before merge. Next useful batch starts with `com.sony.dtv.promos`; skip unchanged
holds until due or material evidence arrives.
