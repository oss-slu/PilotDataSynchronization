- `xplane_plugin`: X-Plane's data extraction plugin
- `relay`: External GUI program that takes the data extracted by `xplane_plugin` and sends it to iMotions over TCP
- `baton`: C++ library for communicating with `relay`. Provides an object that handles inter-process communication (IPC) and abstracts over it for ease-of-use. Currently located in `xplane_plugin/subprojects`

To review, the data flow is as follows:
- `xplane_plugin` extracts the data
- `xplane_plugin` uses the `baton` library to send the data to the `relay` program
- `relay` sends the data to iMotions

`relay` and `baton` are developed by this team and are not external programs/libraries.

`xplane_plugin` and `relay` are top level subprojects in the repo. It is currently undecided whether `baton` will be top level or not, and so for now will be found under `xplane_plugin/subprojects/`. In the event that we begin work on Prepar3D during this iteration, we can decide if `baton` is generic enough to use for both simulators and move it to the top level, or have a `baton` version for each simulator plugin as an internal dependency.
