# Ambiente di build dell'app Android: JDK 17, Android SDK e Gradle. Niente toolchain sull'host.
# Uso: scripts/gradle.sh assembleDebug
FROM eclipse-temurin:17-jdk-jammy
ENV ANDROID_HOME=/opt/android-sdk ANDROID_SDK_ROOT=/opt/android-sdk GRADLE_VERSION=8.11.1
RUN apt-get update && apt-get install -y --no-install-recommends unzip wget ca-certificates && rm -rf /var/lib/apt/lists/*
RUN mkdir -p $ANDROID_HOME/cmdline-tools && cd /tmp \
 && wget -q https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip -O tools.zip \
 && unzip -q tools.zip -d $ANDROID_HOME/cmdline-tools && mv $ANDROID_HOME/cmdline-tools/cmdline-tools $ANDROID_HOME/cmdline-tools/latest \
 && rm tools.zip
RUN yes | $ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager --licenses >/dev/null \
 && $ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager "platforms;android-35" "build-tools;35.0.0" "platform-tools" >/dev/null
RUN cd /opt && wget -q https://services.gradle.org/distributions/gradle-${GRADLE_VERSION}-bin.zip -O g.zip \
 && unzip -q g.zip && rm g.zip && ln -s /opt/gradle-${GRADLE_VERSION}/bin/gradle /usr/local/bin/gradle
RUN useradd -m -u 1000 builder || true
