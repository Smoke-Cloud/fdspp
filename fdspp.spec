Name:           fdspp
Version:        0.2.6
Release:        1%{?dist}
Summary:        FDS Pre-Processor

License:        AllRightsReserved
Source0:        fdspp-%{version}.tar.gz
Url:            https://smokecloud.io

BuildRequires:  systemd, systemd-rpm-macros
Requires:       bash

%description
A pre-processor for FDS (Fire Dynamics Simulator).

%prep
%setup -n fdspp-%{version}

%global debug_package %{nil}
%build
cargo build --release

%install
rm -rf $RPM_BUILD_ROOT
install -D target/release/fdspp $RPM_BUILD_ROOT/%{_bindir}/fdspp

%files
%{_bindir}/fdspp

%changelog
* Sat Dec 18 2021 admin
-
