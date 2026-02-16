# Build and Deployment Instructions

This project is a Solana program using the **Arcium** framework for confidential computing. It consists of two main parts:
1.  **Encrypted Instructions** (`encrypted-ixs/`): Rust code that defines the confidential logic (compiled to `.arcis` files).
2.  **Solana Program** (`programs/horsebetting/`): The Anchor program that orchestrates the logic on-chain.

## Prerequisites

Ensure you have the following installed:
-   **Rust**: `rustc`, `cargo`
-   **Solana CLI**: `solana-cli`
-   **Anchor CLI**: `avm`, `anchor`
-   **Node.js** & **Yarn**
-   **Arcium CLI**: You must have the `arcium` CLI installed.

## 1. Installation

First, install the project dependencies:

```bash
yarn install
```

## 2. Building

You need to build both the encrypted instructions (circuits) and the Solana program.

### Step 2.1: Build Encrypted Instructions
This command compiles the Rust code in `encrypted-ixs/` into `.arcis` files (e.g., `build/add_together.arcis`) which are required by the program and tests.

```bash
arcium build
```

*Note: This will likely create a `build/` directory in the project root containing the `.arcis` files.*

### Step 2.2: Build Solana Program
Compile the Anchor program.

```bash
anchor build
```

## 3. Testing

To test the program, use the `arcium` CLI, which sets up the full environment (Solana localnet + Arcium MXE nodes).

```bash
arcium test
```

Or if you want to run specific tests or use Anchor directly (ensure your local validator and Arcium nodes are running):

```bash
anchor test
```

## 4. Deployment

### Localnet
The `arcium test` command usually handles the ephemeral deployment for testing.

### Devnet / Mainnet
To deploy to a persistent network:

1.  **Configure environment**: Ensure your `Anchor.toml` and `Arcium.toml` are configured for the target cluster.
2.  **Build**:
    ```bash
    arcium build
    anchor build
    ```
3.  **Deploy**:
    ```bash
    # Deploy the Solana program
    anchor deploy --provider.cluster devnet
    
    # Initialize the MXE (Multi-Party Execution) environment if necessary
    arcium init-mxe
    ```

*Refer to the `Arcium.toml` file for cluster configurations and offsets.*
