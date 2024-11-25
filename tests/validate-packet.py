import socket


def validate_packet(packet):
    # Validate UTF-8 encoding
    try:
        packet_utf8 = packet.decode("utf-8")
    except UnicodeDecodeError:
        print("Error: Packet is not UTF-8 encoded.")
        return False
    
    # Validate end-of-line terminator (\r\n)
    if not packet_utf8.endswith("\r\n"):
        print("Error: Packet does not end with '\\r\\n'.")
        return False
    
    # Remove the \r\n for easier parsing of fields
    packet_utf8 = packet_utf8.strip()
    
    # Split the packet into fields using the semicolon delimiter
    fields = packet_utf8.split(";")
    
    # Check if required fields are present
    required_fields = ["event_source", "timestamp", "flight_data"]
    field_names = ["event_source", "timestamp", "flight_data"]  # Expected field names
    
    if len(fields) < len(required_fields):
        print("Error: Missing fields. Expected fields:", required_fields)
        return False
    
    # Validate field presence and display the packet
    for i, field_name in enumerate(field_names):
        if i >= len(fields) or not fields[i].strip():
            print(f"Error: Missing required field '{field_name}'")
            return False
    
    print("Packet received and validated successfully:")
    print(f"  Event Source: {fields[0]}")
    print(f"  Timestamp: {fields[1]}")
    print(f"  Flight Data: {fields[2]}")
    return True


def start_client(host, port):
    # Connect to the server
    client_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    client_socket.connect((host, port))
    
    print("Connected to server. Listening for packets...")

    try:
        while True:
            # Receive data from the server in 1024-byte chunks
            packet = client_socket.recv(1024)
            if not packet:
                break  # No more data from the server
            
            # Validate received packet
            is_valid = validate_packet(packet)
            if is_valid:
                print("Packet passed all validations.\n")
            else:
                print("Packet failed validation.\n")
    except Exception as e:
        print("Error:", e)
    finally:
        client_socket.close()
        print("Connection closed.")


if __name__ == "__main__":
    # Start the client to listen for packets
    start_client("127.0.0.1", 8089)
