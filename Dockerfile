FROM mcr.microsoft.com/devcontainers/rust:0-1-bullseye

RUN apt-get update && apt-get install -y libgtk-4-bin libgtk-4-common libgtk-4-dev libgstreamer1.0-dev