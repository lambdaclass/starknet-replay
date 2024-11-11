from argparse import ArgumentParser

argument_parser = ArgumentParser('Stress Test Plotter')
argument_parser.add_argument("native_logs_path")
argument_parser.add_argument("vm_logs_path")
argument_parser.add_argument("output")
arguments = argument_parser.parse_args()

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

pd.set_option('display.max_colwidth', None)
sns.set_color_codes("bright")


# Top 100 most common classes, without non significant zeroes
top_classes_list=[
    "0x279d12a282d7888e3fdbe456150775be2c160e7c78d409bbf02be68fdf275ce",
    "0x0e2eb8f5672af4e6a4e8a8f1b44989685e668489b0a25437733756c5a34a1d6",
    "0x7f3777c99f3700505ea966676aac4a0d692c2a9f5e667f4c606b51ca1dd3420",
    "0x5aa807b26b529e9f0c802a84020983ecb0ce92c1e2768b2a97b250cc268a393",
    "0x30e64ecb769ff832478ed5ce52fc5b81ffc7d32dd36cd9b8937135683339a2c",
    "0x59e4405accdf565112fe5bf9058b51ab0b0e63665d280b816f9fe4119554b77",
    "0x5ffbcfeb50d200a0677c48a129a11245a3fc519d1d98d76882d1c9a1b19c6ed",
    "0x4ad3c1dc8413453db314497945b6903e1c766495a1e60492d44da9c2a986e4b",
    "0x306288971002bd7906e3a607d504dfb28dcbdc7655a115984e567dce3b67e8f",
    "0x3eda3f87b4df8abfcd6fa95ae034dda9836fb66ec585f4f0b5cdb7636cac927",
    "0x082d2925503eacb6834343808346c35d01bf397b81aecb3e58624731e86b4a9",
    "0x6f4515274ee23404789c3351a77107d0ec07508530119822046600ca6948d6e",
    "0x36078334509b514626504edc9fb252328d1a240e4e948bef8d0c08dff45927f",
    "0x0816dd0297efc55dc1e7559020a3a825e81ef734b558f03c83325d4da7e6253",
    "0x61d4915fb37b0ba3d4fbcd1a3369f67c811670fcbfc6ea34df8250d952ba980",
    "0x518051ab08e1322d16f153a75bb4de85690914d7d3d18245698d4543a93df06",
    "0x5fdb47de4edfd5983f6a82a1beeb1aab2b69d3674b90730aa1693d94d73f0d3",
    "0x5dde112c893e2f5ed85b92a08d93cfa5579ce95d27afb34e47b7e7aad59c1c0",
    "0x1a736d6ed154502257f02b1ccdf4d9d1089f80811cd6acad48e6b6a9d1f2003",
    "0x74aad3c412b1d7c05f720abfd39adc709b8bf8a8c7640e50505a9436a6ff0cf",
    "0x29fd83b01f02b45987dfb9652633cd0f1f64a0f36403ab1fed7bd99642fa474",
    "0x6192bd5cfffdcb0cc27d550075a6410a733df4be20fcfb32b834abf011d3fc8",
    "0x6afa2f21a611f8b4a77ef681a9eb0c7cd6e52aa918e7f8b4b8142b4ca1bde49",
    "0x79561bce61f39a0dfab9413cee86f6cfe7d9112b96abce545c6e929b20081eb",
    "0x5e269051bec902aa2bd421d348e023c3893c4ff93de6c5f4b8964cd67cc3fc5",
    "0x6d8610c75a9890781702c7e7df035ad4fd3e94acd17be0cf7ca1d5649b7aea4",
    "0x05e8e4eb307b1342aeba5ba83d5145fcaf74027466e462cb5578bb4c1a93fac",
    "0x1abebea2e4b03680a65489f59a101ebb1becf3111d19b2597443323883d9c3c",
    "0x29a6929f323bb6135fd17e9a71ee341dff9391bb090e1248516c73d6a94e22b",
    "0x6844145a2a7e4cf07eb49923f2565cb7700601bfcc8140a2685d886da6e7e20",
    "0x74340dcef0aaf445dd937f783836c1611f5a76d44cd05a67bdf5b5f47fd48fa",
    "0x29927c8af6bccf3f6fda035981e765a7bdbf18a2dc0d630494f8758aa908e2b",
    "0x32e17891b6cc89e0c3595a3df7cee760b5993744dc8dfef2bd4d443e65c0f40",
    "0x02f6d77cb0bca422706a91858dff62975aef4b8214520aadb1f0b39c51f5fde",
    "0x08fade1a36f2bfcaa55b53c96dfb615e8e60110b87765cf449d09b6e0397b17",
    "0x50d220a0f76ed0e1ff2df6b433190c9cc360ccc08c4e57d6037c7c2dfd62a91",
    "0x137a5970f752fd80649fb38d684b090decebec4f2853396617e4f2f01e5396a",
    "0x6d3daf30cf6ea68549a2a3c8fe384ac66da95bd31e4a6afde9e04dfd5dca1ea",
    "0x5ee939756c1a60b029c594da00e637bf5923bf04a86ff163e877e899c0840eb",
    "0x10addfea030980d47e2438946e1fe91acd9132b7138b6c0dd8be3a7bc7394e7",
    "0x230f20832d73433a0ef69663545aabb607a3bbc0ff0a5f75e5d9716730e4141",
    "0x39bb4d21b2faebdf6c7db4eef6ba65bc652ecef957f1ad2fca0ab4c6d559e50",
    "0x58c76a4d410272edf54033221efdbe5313b3744306847e17b3c4eb5597aebb8",
    "0x06a54af2934978ac59b27b91291d3da634f161fd5f22a2993da425893c44c64",
    "0x63ee878d3559583ceae80372c6088140e1180d9893aa65fbefc81f45ddaaa17",
    "0x3c8904d062171ab62c1f2e52e2b33299c305626a8f8a253a1544a6ad774121b",
    "0x0009e6d3abd4b649e6de59bf412ab99bc9609414bbe7ba86b83e09e96dcb120",
    "0x4f9849485e35f4a1c57d69b297feda94e743151f788202a6d731173babf4aec",
    "0x1bb734916a18c74772a8e1b748e600be4d8d8987a94bf98e4b905e73fcdfbbf",
    "0x77a5cf138575adea2a0de34c51bede28a7a251bbfcc6e02bdb8a4bd2ef6fc9e",
    "0x7a146055644f1f7c1103ba9ba53b94aeada50ebfdb1b763e0fb2e5de4b08b8d",
    "0x4225fcf6bf42a54dec4e9b6d3e7d1b5eb844f5ba9694058ef21826fd00cdba1",
    "0x5431265f9d2416426da800a23ddd3fe33db8e2b9fe96dbc48588ac3ac70c091",
    "0x7074d9c4ee72049ceda4e3c1d3246744caae55dbcf61b78526832bc899055ef",
    "0x17215d1cb473fe6adbe628d9c3a743830e380ef1867cc018a45e838c6468eb1",
    "0x76b533a863ea90cc391c5226ec5d5b6466bfeb0721d6c970d3b627a18de720d",
    "0x337f32bd5fccef2b67d2c620a8d31abccd5960c37f0c0b58d5e5327fac7e6b7",
    "0x24a9edbfa7082accfceabf6a92d7160086f346d622f28741bf1c651c412c9ab",
    "0x25a7d7c63acb6458b3d53c7ccf05fa8d5d3374993a30121db32e13623899860",
    "0x02e4fbe1c458eb6ec7a5b15d78674bb6fbfd5872aca0fe920d462bd6fb8ed0b",
    "0x1831f0668217db340ae63aa0f798aad6d58f81d30fef06f775f69d24e41b363",
    "0x4db5a97dcb8e229e71d741496840fe8ba4af1c1adbeef7743beb65a8f467f4c",
    "0x38fc1d7bfaddf7c26dd686a9909442866dd84ef4a12af3fff63a875e45c076d",
    "0x06c77d54bfca18537162f6a2c67db81783c6c414edb68a1117b56b1e48b9ec8",
    "0x38082dfcc9e1afd67eaabcb9b0cff0646237badf22a494bcfc72532c8fc2249",
    "0x1cb5e128a81be492ee7b78cf4ba4849cb35f311508e13a558755f4549839f14",
    "0x386d92966f0908ded021f3959610dc52a605922387e0847df45832c046a79bd",
    "0x7fb70339df89a45968717ecfbb778d8d55c5072e3eb70a4e90a027858b3d99e",
    "0x6551d5af6e4eb4fac2f9fea06948a49a6d12d924e43a63e6034a6a75e749577",
    "0x0b229aee04c4b886d3e32497fca25e7faebc78107f7d8788be30539cacf50b9",
    "0x2cd3c16a0112b22ded4903707f268125fcf46fd7733761e62c13fc0157afd8d",
    "0x1cb96b938da26c060d5fd807eef8b580c49490926393a5eeb408a89f84b9b46",
    "0x1c72693d81fbd5323401c0cd0200e973335766acebd207bc5a6b9199716f6c2",
    "0x5400e90f7e0ae78bd02c77cd75527280470e2fe19c54970dd79dc37a9d3645c",
    "0x2580632f25bbee45da5a1a1ef49d03a984f78dde4019069aa9f25aac06f941a",
    "0x26fe8ea36ec7703569cfe4693b05102940bf122647c4dbf0abc0bb919ce27bd",
    "0x25ec026985a3bf9d0cc1fe17326b245dfdc3ff89b8fde106542a3ea56c5a918",
    "0x33434ad846cdd5f23eb73ff09fe6fddd568284a0fb7d1be20ee482f044dabe2",
    "0x16f2e4271df6214026a7c669d0e795b05667b9d7e349b9a9565bd7784653ca9",
    "0x7b5cd6a6949cc1730f89d795f2442f6ab431ea6c9a5be00685d50f97433c5eb",
    "0x2b39bc3f4c1fd5bef8b7d21504c44e0da59cf27b350551b13d913da52e40d3b",
    "0x4247b4b4eef40ec5d47741f5cc911239c1bbd6768b86c240f4304687f70f017",
    "0x562fc1d911530d18a86ea3ef4be50018923898d3c573288c5abb9c2344459ed",
    "0x6038c772adbf8740b1be1a75853b8d1ecc69621c938d186c80111176bafb1a2",
    "0x10f037591f881e9bc721f34d1bf12867aad74c7b01a5585b7e7cc1112e2627c",
    "0x24e63f31971967be4051165743998fb72513923126b7be45035e94bacb050a7",
    "0x220fafd2163c285f26a981772d96b0ce130ae1e4502ce45cc127ab87df295b0",
    "0x4bd71429c6b1803ff6d77944af2c23c26d1bf0ae34aa257916910c484f4d087",
    "0x3f63cecdc4964acafb921ba2934c6507d1b3c344edb64c2762cf08053169ab9",
    "0x2eb0fc47912fe0997d82d8c66aad672cfd8b3ec56161d42e3059c3443603f71",
    "0x322e6fc040c1985e5239d79838736a09f9f8cdce5db3336770c8c97f93c3a61",
    "0x5807986d79d4ba7e8729baaad831399979ac5ef9dd516508e0c27cfaae62620",
    "0x182dfcf12cf38789f5937a1b920f0513195131a408716224ac8273f371d9d0a",
    "0x0589a40e9cd8784359c066db1adaf6cf0d92322ce579fc0c19739649beae132",
    "0x26b25c3a9bf7582cc8a9e6fff378cb649fc5cba404f93633ed41d59053dcd31",
    "0x73fc0a8c1fae1b2b42762cc0cc79337e57aa438e6bab44aab2c46733f5f0405",
    "0x364f8f46d2fbdc1593e75e291da5e830018f8ad6432f45fe2e2924fd53f94b4",
    "0x3a350cc2540d8c608feafe3d337291776a1f02a3a640fc3a4e4a6160a608a0e",
    "0x231adde42526bad434ca2eb983efdd64472638702f87f97e6e3c084f264e06f",
    "0x4231e8125da430bdec5ad18810528fbc520db9984a7ef4a890b0984c8eadf2a",
]

# Set to empty list to disable filtering
classes_list = (
    []
    + top_classes_list
)
# convert to integer set to compare faster
classes = set(map(lambda x: int(x, 16), classes_list))

def filter(row):
    if len(classes_list) == 0:
        return True

    class_hash_dec = int(row["class hash"], 0)
    return class_hash_dec in classes

def load_dataset(path):
    dataset = pd.read_json(path, lines=True, typ="series").apply(pd.Series)
    return dataset[dataset.apply(filter, axis=1)]

datasetNative = load_dataset(arguments.native_logs_path)
datasetVM = load_dataset(arguments.vm_logs_path)

# CALCULATE MEAN
datasetNative = datasetNative.groupby("class hash").agg(["mean","size"])
datasetVM = datasetVM.groupby("class hash").agg(["mean","size"])
dataset = datasetNative.join(datasetVM, lsuffix="_native", rsuffix="_vm")
dataset.columns = dataset.columns.map('_'.join)

# CALCULATE SPEEDUP
dataset["speedup"] = dataset["time_vm_mean"] / dataset["time_native_mean"]
print("Average Speedup: ", dataset["speedup"].mean())

# FILTER WORST AND BEST
datasetL = dataset.nlargest(20, "speedup")
datasetS = dataset.nsmallest(20, "speedup")
dataset = pd.concat([datasetL, datasetS])
dataset = dataset.drop_duplicates()

# SORT BY POPULARITY OR SPEEDUP
classesIdx= dict(zip(classes_list, range(len(classes_list))))
dataset['order'] = dataset.index.map(classesIdx)
dataset.sort_values(['order', 'speedup'], ascending=[True, False], inplace=True)
dataset.drop('order', axis=1, inplace=True)

print(dataset)

def format_hash(class_hash):
    return f"{class_hash[:6]}..."

figure, axes = plt.subplots(1, 2)

ax=axes[0]

sns.barplot(ax=ax, y="class hash", x="time_vm_mean", data=dataset, formatter=format_hash, label="VM Execution Time", color="r", alpha = 0.75) # type: ignore
sns.barplot(ax=ax, y="class hash", x="time_native_mean", data=dataset, formatter=format_hash, label="Native Execution Time", color="b", alpha = 0.75) # type: ignore

ax.set_xlabel("Mean Time (ns)")
ax.set_ylabel("Class Hash")
ax.set_title("Mean time by Contract Class")

ax=axes[1]

sns.barplot(ax=ax, y="class hash", x="speedup", data=dataset, formatter=format_hash, label="Execution Speedup", color="b", alpha = 0.75) # type: ignore

ax.set_xlabel("Speedup")
ax.set_ylabel("Class Hash")
ax.set_title("Speedup by Contract Class")

figure.savefig(arguments.output)

plt.show()
