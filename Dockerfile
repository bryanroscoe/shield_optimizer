FROM ubuntu:22.04

# Install dependencies
RUN apt-get update && apt-get install -y \
    wget \
    iproute2 \
    iputils-ping \
    libicu70 \
    && rm -rf /var/lib/apt/lists/*

# Install PowerShell - detect architecture and download appropriate binary
RUN ARCH=$(dpkg --print-architecture) && \
    if [ "$ARCH" = "arm64" ]; then \
        PS_ARCH="linux-arm64"; \
    else \
        PS_ARCH="linux-x64"; \
    fi && \
    wget -q "https://github.com/PowerShell/PowerShell/releases/download/v7.4.7/powershell-7.4.7-${PS_ARCH}.tar.gz" -O /tmp/powershell.tar.gz && \
    mkdir -p /opt/microsoft/powershell/7 && \
    tar -xzf /tmp/powershell.tar.gz -C /opt/microsoft/powershell/7 && \
    chmod +x /opt/microsoft/powershell/7/pwsh && \
    ln -s /opt/microsoft/powershell/7/pwsh /usr/bin/pwsh && \
    rm /tmp/powershell.tar.gz

WORKDIR /app
CMD ["pwsh"]
