#!/bin/bash

# Generate Android Keystore using Docker
# No Java installation required!

set -e

echo "=========================================="
echo "Android Keystore Generator"
echo "=========================================="
echo ""

# Prompt for inputs
read -p "Enter keystore password (save this!): " KEYSTORE_PASSWORD
read -p "Enter key alias (default: mysoloband): " KEY_ALIAS
KEY_ALIAS=${KEY_ALIAS:-solobandultra}
read -p "Enter key password (press Enter to use same as keystore): " KEY_PASSWORD
KEY_PASSWORD=${KEY_PASSWORD:-$KEYSTORE_PASSWORD}

echo ""
read -p "Enter your organization name (default: SoloBandUltra): " ORG_NAME
ORG_NAME=${ORG_NAME:-SoloBandUltra}

echo ""
echo "Generating keystore with Docker..."
echo ""

# Generate keystore using Docker
docker run --rm -v "$PWD":/work -w /work openjdk:27-ea-slim \
  keytool -genkeypair -v \
  -storetype PKCS12 \
  -keystore release.keystore \
  -alias "$KEY_ALIAS" \
  -keyalg RSA \
  -keysize 2048 \
  -validity 10000 \
  -storepass "$KEYSTORE_PASSWORD" \
  -keypass "$KEY_PASSWORD" \
  -dname "CN=$ORG_NAME, OU=Development, O=$ORG_NAME, L=Cary, ST=NC, C=US"

echo ""
echo "✅ Keystore created: release.keystore"
echo ""

# Verify keystore
echo "Verifying keystore..."
docker run --rm -v "$PWD":/work -w /work openjdk:21-slim \
  keytool -list -v -keystore release.keystore -storepass "$KEYSTORE_PASSWORD" | head -20

echo ""
echo "=========================================="
echo "Encoding to Base64..."
echo "=========================================="

# Encode to base64 (no line breaks)
if [[ "$OSTYPE" == "darwin"* ]]; then
  # macOS
  base64 -i release.keystore | tr -d '\n' > keystore_base64.txt
else
  # Linux
  base64 -w 0 release.keystore > keystore_base64.txt
fi

echo ""
echo "✅ Base64 encoded: keystore_base64.txt"
echo ""

# Display instructions
echo "=========================================="
echo "GITHUB SECRETS - Add these to your repo:"
echo "=========================================="
echo ""
echo "Secret Name: ANDROID_KEYSTORE_BASE64"
echo "Value: (see keystore_base64.txt or below)"
echo ""
echo "Secret Name: ANDROID_KEYSTORE_PASSWORD"
echo "Value: $KEYSTORE_PASSWORD"
echo ""
echo "Secret Name: ANDROID_KEY_ALIAS"
echo "Value: $KEY_ALIAS"
echo ""
echo "Secret Name: ANDROID_KEY_PASSWORD"
echo "Value: $KEY_PASSWORD"
echo ""
echo "=========================================="
echo "Base64 Content (copy this):"
echo "=========================================="
cat keystore_base64.txt
echo ""
echo ""
echo "=========================================="
echo "IMPORTANT:"
echo "=========================================="
echo "1. Copy keystore_base64.txt content to GitHub secret: ANDROID_KEYSTORE_BASE64"
echo "2. Add the passwords as secrets (shown above)"
echo "3. BACKUP release.keystore somewhere safe (e.g., password manager)"
echo "4. Delete keystore_base64.txt after adding to GitHub (security)"
echo "5. NEVER commit release.keystore to git!"
echo "6. You MUST use this same keystore for ALL future app updates!"
echo "=========================================="
echo ""

# Offer to copy to clipboard (macOS only)
if [[ "$OSTYPE" == "darwin"* ]]; then
  read -p "Copy base64 to clipboard? (y/n): " COPY_CLIPBOARD
  if [[ "$COPY_CLIPBOARD" == "y" ]]; then
    cat keystore_base64.txt | pbcopy
    echo "✅ Copied to clipboard!"
  fi
fi

echo ""
echo "Done! 🎉"
