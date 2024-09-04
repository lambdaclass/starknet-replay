cargo b --release

# https://github.com/lambdaclass/cairo_native/issues/739

echo "--- Testing issue #739 ---"

./target/release/replay tx 0x548c30d3ee15d5180c1cde487a7c4a66efd40dc8c3bad1afe9f98069523dcd4 mainnet 657887
./target/release/replay tx 0xd4e73c48b38d8394593c15b5c142d6d38ff16acf774453ae9b0f20dfc32c82 mainnet 657887
./target/release/replay tx 0x1b2c498c06c6c3354c74d09dbc38de295cde904a2ef9bf10f8222ef9381be60 mainnet 659392

echo "--- Testing issue #714 ---"

# https://github.com/lambdaclass/cairo_native/issues/714

./target/release/replay tx 0x00a1e8372b6de461e939b63d7d2a7c4a60bc333cae92a9e0800a575e13f202f7 mainnet 646356


# https://github.com/lambdaclass/cairo_native/issues/722

echo "--- Skipping #722 (OOM) ---"

# ./target/release/replay tx 0x6c51758aa1ae9506602fffb9194da427fe948314b74eb93cdc9570558d4a88d mainnet 648461


echo "--- Testing issue #727 ---"

# https://github.com/lambdaclass/cairo_native/issues/727

./target/release/replay tx 0x7286e45814889d2dc7dbbf26c5b96b4950280c3fd3f2e38a44d8239a00db336 mainnet 626173

echo "--- Testing issue #738 ---"

# https://github.com/lambdaclass/cairo_native/issues/738

./target/release/replay tx 0x5ba52f32a6a5add8affdd8c7caece321fe76c76e036c49b40db227154916353 mainnet 657887
./target/release/replay tx 0x75c5631aefd35cc4f7bf5f245f10a726336219f35c647b3434353722bdcc23b mainnet 657887

echo "--- Testing issue #740 ---"

# https://github.com/lambdaclass/cairo_native/issues/740

./target/release/replay tx 0x02a23e4131787cc48259d25ea7940c2b65c2f2ca81532cd5df3fc5a0ffcd8ec5 mainnet 656398
