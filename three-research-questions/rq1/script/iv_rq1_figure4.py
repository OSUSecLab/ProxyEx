import numpy as np
import matplotlib.pyplot as plt


# Project and file paths
project_path = "../data/"
tx_file_list = project_path + "iv_rq1_figure4.txt"

# Open the file and extract tx count
total_tx_count_list = []
with open(tx_file_list, "r") as f:
    for line in f:
        addr_details = line.split()
        tx_count = int(addr_details[1])
        total_tx_count_list.append(tx_count)

# Add 40 zeros to the list to include proxies with 0 tx count
for i in range(0, 40):
    total_tx_count_list.append(0)
    i += 1

print("Length of total tx count list ====> ", len(total_tx_count_list))

# Sort the data and only include values less than 100
total_tx_count_list = np.sort(total_tx_count_list)
total_tx_count_list = total_tx_count_list[total_tx_count_list <= 100]

# Designing the plot
plt.rcParams['axes.unicode_minus'] = False
plt.rcParams.update({'font.size': 36})
fig, ax1 = plt.subplots(figsize=(16, 9))

# Calculate counts and unique values
unique_values, counts = np.unique(total_tx_count_list, return_counts=True)

# Computing the CDF
pdf = counts / np.sum(counts)
cdf = np.cumsum(pdf)

# Plotting the bar chart
ax1.bar(unique_values, counts,  color='r', alpha=0.3)
ax1.ticklabel_format(style='plain')
ax1.set_xlabel('Transaction Count', fontdict={'fontsize': 36})
ax1.set_xlim(-1, 101)
ax1.set_ylabel('Proxy Count', fontdict={'fontsize': 36})
ax1.set_yscale('log')
ax1.set_ylim(0, max(counts) * 1.8)

# Prepend a data point at (0, 0)
unique_values = np.insert(unique_values, 0, 0)
cdf = np.insert(cdf, 0, 0)

# Plotting the CDF
ax2 = ax1.twinx()
ax2.plot(unique_values, cdf, label="CDF", color="b")
ax2.set_ylabel('CDF', fontdict={'fontsize': 36})
ax2.set_yticklabels([0, 0.2, 0.4, 0.6, 0.8, 1.0])
ax2.set_ylim(0, 1.05)

# Save the plot
plt.savefig(project_path + "iv_rq1_figure4.pdf",
            facecolor='white', bbox_inches='tight')

# Show the plot
plt.show()

# End of Program
