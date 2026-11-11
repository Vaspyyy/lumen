# Wraps a lumen that is already built. packaging/package.sh calls rpmbuild
# with lumen_version, and with lumen_tree: the folder that holds the files.

# nothing is compiled here, so there is nothing to strip or to take apart
%global debug_package %{nil}
%global __os_install_post %{nil}
%global _build_id_links none
# xz: every system with RPM unpacks it
%global _binary_payload w6.xzdio

Name:           lumen-terminal
Version:        %{lumen_version}
Release:        1
Summary:        A terminal emulator that understands what your tools print
License:        GPL-3.0-or-later
URL:            https://vaspyyy.github.io/lumen/

# lumen loads these when it starts, so RPM doesn't find them in the program
Requires:       libvulkan.so.1()(64bit)
Requires:       libEGL.so.1()(64bit)
Requires:       libwayland-client.so.0()(64bit)
Requires:       libwayland-egl.so.1()(64bit)
# fc-match tells lumen which fonts the system has
Requires:       fontconfig
Recommends:     (wl-clipboard or xclip)

%description
lumen is a GPU-rendered terminal for Linux, on Wayland and X11, with tabs
and splits built in. A file name in a listing is the file: a click copies
it, Ctrl+Click opens it. Pictures a program sends are drawn in place.

%install
mkdir -p %{buildroot}
cp -a %{lumen_tree}/. %{buildroot}/

%files
%{_bindir}/lumen
%{_datadir}/applications/lumen.desktop
%{_datadir}/icons/hicolor/*/apps/lumen.png
%doc %{_datadir}/doc/lumen-terminal
%license %{_datadir}/licenses/lumen-terminal
