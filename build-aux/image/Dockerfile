FROM registry.fedoraproject.org/fedora-minimal:32

RUN microdnf install glib2-devel gtk3-devel cairo-devel pango-devel \
	meson ninja-build libappstream-glib desktop-file-utils gcc \
	mingw64-gcc mingw64-pango mingw64-poppler mingw64-gtk3 \
	mingw64-winpthreads mingw64-winpthreads-static git \
	&& microdnf clean all

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs > rust-init.sh \
	&& bash rust-init.sh -y --profile minimal -t x86_64-pc-windows-gnu,x86_64-unknown-linux-gnu \
	&& rm rust-init.sh \
	&& echo "source /root/.cargo/env" > ~/.bashrc

COPY cargo-config /root/.cargo/config

# RUN meson -Dbuildtype=release linux.x86_64 && cd linux.x86_64
# RUN ninja && cd ..  # Builds Linux x86_64 versoin

# RUN meson -Dbuildtype=release windows.x86_64 && cd windows.x86_64
# RUN 
