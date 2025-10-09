import matplotlib.pyplot as plt
import os
import json

CIRCUIT_STATS_DIR_PATH = "./contracts/circuits/contracts2"
STATS_DIR_PATH  = "./contracts/common/contracts_stats"

def gates_vs_compilation_time(directory):
    add_gate = []
    sub_gate = []
    mul_gate = []
    inverse_gate = []
    compilation_times = []
    for entry in os.scandir(directory):  
        path = entry.path
        splited_path = path.split(".")
        if splited_path[2] != "stats":
            continue
        with open(path, "r") as f:
            json_f = json.load(f)
            gates = json_f["sierra_circuit_gates_count"]
            add_gate_freq = gates.get("AddGate", 0)
            sub_gate_freq = gates.get("SubGate", 0)
            mul_gate_freq = gates.get("MulGate", 0)
            inverse_gate_freq = gates.get("InverseGate", 0)
            curr_compilation_time = json_f["compilation_total_time_ms"]

            add_gate.append(add_gate_freq)
            sub_gate.append(sub_gate_freq)
            mul_gate.append(mul_gate_freq)
            inverse_gate.append(inverse_gate_freq)
            compilation_times.append(curr_compilation_time)
    
    fig, axs = plt.subplots(2, 2)
    axs[0,0].scatter(add_gate, compilation_times)
    axs[0,0].set_title("Add Gate")

    axs[0,1].scatter(sub_gate, compilation_times)
    axs[0,1].set_title("Sub Gate")

    axs[1,0].scatter(mul_gate, compilation_times)
    axs[1,0].set_title("Mul Gate")

    axs[1,1].scatter(inverse_gate, compilation_times)
    axs[1,1].set_title("Inverse Gate")

    for ax in axs.flat:
        ax.set(xlabel='Gate Quantity', ylabel='Time (ms)')

    fig.tight_layout(pad=2.0)

    plt.show()

def circuits_vs_compilation_time(directory):
    circuits_quantity = []
    compilation_times = []
    for entry in os.scandir(directory):  
        path = entry.path
        splited_path = path.split(".")
        if splited_path[2] != "stats":
            continue
        with open(path, "r") as f:
            json_f = json.load(f)
            curr_circuits_quantity = json_f["sierra_circuits_count"]
            curr_compilation_time = json_f["compilation_total_time_ms"]

            circuits_quantity.append(curr_circuits_quantity)
            compilation_times.append(curr_compilation_time)
    
    plt.scatter(circuits_quantity, compilation_times)
    plt.suptitle("Circuits Quantity vs Compilation Time")
    plt.xlabel("Circuits quantity")
    plt.ylabel("Milliseconds")
    plt.show()

def parameters_sizes_vs_compilation_time(directory):
    avg_parameters_sizes = []
    max_parameters_sizes = []
    compilation_times = []
    for entry in os.scandir(directory):  
        path = entry.path
        splited_path = path.split(".")
        if splited_path[2] != "stats":
            continue
        with open(path, "r") as f:
            json_f = json.load(f)
            curr_avg = json_f["sierra_avg_params_size"]
            curr_max = json_f["sierra_max_params_size"]
            curr_compilation_time = json_f["compilation_total_time_ms"]
            
            avg_parameters_sizes.append(curr_avg)
            max_parameters_sizes.append(curr_max)
            compilation_times.append(curr_compilation_time)
    
    fig, (ax1, ax2) = plt.subplots(1, 2)
    ax1.scatter(avg_parameters_sizes, compilation_times)
    ax1.set_title("Avg parameters size")
    # ax1.set_xlim([0,500])
    # ax1.set_ylim([0,100000])


    ax2.scatter(max_parameters_sizes, compilation_times)
    ax2.set_title("Max parameters size")
    # ax2.set_xlim([0,3000])
    # ax2.set_ylim([0,100000])

    fig.tight_layout(pad=2.0)
    fig.supxlabel("Bytes")
    fig.supylabel("Milliseconds")
    fig.suptitle("Params sizes vs compilation times")

    plt.show()

# def types_freqs_vs_compilation_times(directory):
#     avg_parameters_sizes = []
#     max_parameters_sizes = []
#     compilation_times = []
#     for entry in os.scandir(directory):  
#         path = entry.path
#         splited_path = path.split(".")
#         if splited_path[2] != "stats":
#             continue
#         with open(path, "r") as f:
#             json_f = json.load(f)

def funcs_quantity(directory):
    funcs_quantity = {}
    for entry in os.scandir(directory):  
        path = entry.path
        splited_path = path.split(".")
        if splited_path[2] != "stats":
            continue
        with open(path, "r") as f:
            json_f = json.load(f)
            quant = json_f["sierra_func_count"]
            funcs_quantity[quant] = funcs_quantity.get(quant, 0) + 1

    plt.bar(list(funcs_quantity.keys()), list(funcs_quantity.values()))
    plt.xlim([0, 200])
    plt.show()

def llvm_func_params_vs_compilation_time(directory):
    max_params_quantity = []
    compilation_times = []
    for entry in os.scandir(directory):  
        path = entry.path
        splited_path = path.split(".")
        if splited_path[2] != "stats":
            continue
        with open(path, "r") as f:
            json_f = json.load(f)
            params_quant = json_f["llvmir_max_functions_params"]
            curr_compilation_time = json_f["compilation_total_time_ms"]

            compilation_times.append(curr_compilation_time)
            max_params_quantity.append(params_quant)
    
    plt.scatter(max_params_quantity, compilation_times)
    plt.suptitle("LLVM IR Max Params Quantity vs Compilation Time")
    plt.xlabel("Max params quantity")
    plt.ylabel("Milliseconds")
    plt.show()

def total_gates_vs_compilation_time(directory):
    gates_counts = []
    compilation_times = []
    for entry in os.scandir(directory):  
        path = entry.path
        splited_path = path.split(".")
        if splited_path[2] != "stats":
            continue
        with open(path, "r") as f:
            json_f = json.load(f)
            try:
                gates = json_f["sierra_circuit_gates_count"]
            except KeyError:
                gates = json_f["statistics"]["sierra_circuit_gates_count"]

            try:
                curr_compilation_time = json_f["compilation_total_time_ms"]
            except KeyError:
                curr_compilation_time = json_f["statistics"]["compilation_total_time_ms"]

            gates_total_count = sum(gates.values())
            gates_counts.append(gates_total_count)
            compilation_times.append(curr_compilation_time)
    
    plt.scatter(gates_counts, compilation_times)
    plt.suptitle("Total of gates vs Compilation Time")
    plt.xlabel("Total of Gates")
    plt.ylabel("Milliseconds")
    plt.show()

def type_size_vs_as_param_count_sus_contracts(directory):
    as_param_counters = []
    type_sizes = []
    for i, entry in enumerate(os.scandir(directory)):  
        path = entry.path
        splited_path = path.split(".")
        if splited_path[2] != "stats":
            continue
        with open(path, "r") as f:
            json_f = json.load(f)
            # types = json_f["statistics"]["sierra_declared_types_stats"]
            types = json_f["sierra_declared_types_stats"]

            for type_stats in types.values():
                type_size = type_stats["size"]
                type_as_param_counts = type_stats["as_param_count"]

                as_param_counters.append(type_as_param_counts)
                type_sizes.append(type_size)
    
    plt.scatter(as_param_counters, type_sizes)
    # plt.ylim([0, 4000])
    plt.suptitle("Types sizes vs their frequency as libfuncs params")
    plt.xlabel("Times being a param in a libfunc")
    plt.ylabel("Size (Bytes)")
    plt.show()

def func_stat_vs_compilation_time(directory, xstat, xlabel, ystat, ylabel, title):
    stats_data = []
    compilation_times = []
    for entry in os.scandir(directory):  
        path = entry.path
        splited_path = path.split(".")
        if splited_path[2] != "stats":
            continue
        with open(path, "r") as f:
            json_f = json.load(f)
            
            if directory == STATS_DIR_PATH:
                json_f = json_f["statistics"]

            max_stat = 0
            curr_compilation_time = json_f[ystat]
            for val in json_f["sierra_func_stats"].values():
                curr_stat = val[xstat]
                if curr_stat > max_stat:
                    max_stat = curr_stat
            
            stats_data.append(max_stat)
            compilation_times.append(curr_compilation_time)
    
    
    plt.xlabel(xlabel)
    plt.ylabel(ylabel)
    plt.title(title)
    plt.scatter(stats_data, compilation_times)

    plt.show()



# parameters_sizes_vs_compilation_time(CIRCUIT_STATS_DIR_PATH)
# circuits_vs_compilation_time(CIRCUIT_STATS_DIR_PATH)
# gates_vs_compilation_time(CIRCUIT_STATS_DIR_PATH)
# llvm_func_params_vs_compilation_time(CIRCUIT_STATS_DIR_PATH)
# total_gates_vs_compilation_time(CIRCUIT_STATS_DIR_PATH)
# type_size_vs_as_param_count_sus_contracts(CIRCUIT_STATS_DIR_PATH)
# func_stat_vs_compilation_time(CIRCUIT_STATS_DIR_PATH, "params_quant", "Quantity", "compilation_total_time_ms", "Milliseconds", "Max number of parameters in a func vs compile time")
# func_stat_vs_compilation_time(CIRCUIT_STATS_DIR_PATH, "params_total_size", "Bytes", "object_size_bytes", "Bytes", "Max total size of params in a func vs object size")
# func_stat_vs_compilation_time(CIRCUIT_STATS_DIR_PATH, "params_quant", "Quantity", "object_size_bytes", "Bytes", "Max number of parameters in a func vs object size")
# func_stat_vs_compilation_time(CIRCUIT_STATS_DIR_PATH, "return_types_total_size", "Bytes", "compilation_total_time_ms", "Milliseconds", "Max total size of return types in a func vs compile time")
# func_stat_vs_compilation_time(CIRCUIT_STATS_DIR_PATH, "return_types_quant", "Quantity", "object_size_bytes", "Bytes", "Max number of return types in a func vs object size")
# func_stat_vs_compilation_time(CIRCUIT_STATS_DIR_PATH, "return_types_total_size", "Bytes", "object_size_bytes", "Bytes", "Max total size of return types in a func vs object size")
func_stat_vs_compilation_time(CIRCUIT_STATS_DIR_PATH, "return_types_quant", "Quantity", "compilation_total_time_ms", "Milliseconds", "Max number of return types in a func vs compile time")


# gates_vs_compilation_time(STATS_DIR_PATH)
# circuits_vs_compilation_time(STATS_DIR_PATH)
# parameters_sizes_vs_compilation_time(STATS_DIR_PATH)
# llvm_func_params_vs_compilation_time(STATS_DIR_PATH)
# total_gates_vs_compilation_time(STATS_DIR_PATH)
# type_size_vs_as_param_count_sus_contracts(STATS_DIR_PATH)
# func_stat_vs_compilation_time(STATS_DIR_PATH, "params_quant", "Max number of parameters in a func vs compile time", "Quantity", "compilation_total_time_ms")
# func_stat_vs_compilation_time(STATS_DIR_PATH, "params_total_size", "Bytes", "object_size_bytes", "Bytes", "Max total size of params in a func vs object size")
# func_stat_vs_compilation_time(STATS_DIR_PATH, "params_quant", "Quantity", "object_size_bytes", "Bytes", "Max number of parameters in a func vs object size")
# func_stat_vs_compilation_time(STATS_DIR_PATH, "return_types_total_size", "Bytes", "compilation_total_time_ms", "Milliseconds", "Max total size of return types in a func vs compile time")
# func_stat_vs_compilation_time(STATS_DIR_PATH, "return_types_quant", "Quantity", "object_size_bytes", "Bytes", "Max number of return types in a func vs object size")
# func_stat_vs_compilation_time(STATS_DIR_PATH, "return_types_total_size", "Bytes", "object_size_bytes", "Bytes", "Max total size of return types in a func vs object size")
func_stat_vs_compilation_time(STATS_DIR_PATH, "return_types_quant", "Quantity", "compilation_total_time_ms", "Milliseconds", "Max number of return types in a func vs compile time")



# funcs_quantity(STATS_DIR_PATH)
